#!/usr/bin/env python3
"""Independent, bounded linked-unit oracle for the 17 pinned ESP8266 STEP fixtures.

This deliberately interprets only declared representation/context/unit schemas;
other geometric records are syntax-parsed but never used to infer length units.
No CAD accuracy or production-admission claim follows from matching unit graphs.
"""
import argparse
import hashlib
import json
import math
import re
import struct
import subprocess
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MODELS = ROOT / "target/dataset-intake/esp8266-pr6-5283bc0-source-candidate/data/board-sources/esp8266-nodemcu-reference/structured-source/embedded-models"
SOURCE_RECEIPT = ROOT / "target/github-delivery/esp8266-step-preflight/receipt.json"
MAX_BYTES = 32 * 1024 * 1024
MAX_TOKENS = 4_000_000
MAX_ENTITIES = 250_000
MAX_DEPTH = 48
MAX_WORK = 100_000
TOKEN = re.compile(r"\s+|/\*.*?\*/|#[0-9]+|'(?:''|[^'])*'|\"[0-9A-Fa-f]*\"|\.[A-Za-z_][A-Za-z_0-9]*\.|[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[Ee][+-]?[0-9]+)?|[A-Za-z_][A-Za-z_0-9]*|[(),;=$*]", re.S)
REPRESENTATIONS = {
    "REPRESENTATION", "SHAPE_REPRESENTATION", "ADVANCED_BREP_SHAPE_REPRESENTATION",
    "FACETED_BREP_SHAPE_REPRESENTATION", "MANIFOLD_SURFACE_SHAPE_REPRESENTATION",
    "GEOMETRICALLY_BOUNDED_SURFACE_SHAPE_REPRESENTATION", "GEOMETRICALLY_BOUNDED_WIREFRAME_SHAPE_REPRESENTATION",
    "EDGE_BASED_WIREFRAME_SHAPE_REPRESENTATION", "SHELL_BASED_WIREFRAME_SHAPE_REPRESENTATION",
    "TESSELLATED_SHAPE_REPRESENTATION", "TESSELLATED_SHAPE_REPRESENTATION_WITH_ACCURACY_PARAMETERS",
    "DRAUGHTING_MODEL", "DEFINITIONAL_REPRESENTATION",
    "MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION",
}
CONTEXTS = {"REPRESENTATION_CONTEXT", "GEOMETRIC_REPRESENTATION_CONTEXT", "GLOBAL_UNIT_ASSIGNED_CONTEXT", "GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT"}
PREFIX_EXPONENTS = {"EXA": 18, "PETA": 15, "TERA": 12, "GIGA": 9, "MEGA": 6, "KILO": 3, "HECTO": 2, "DECA": 1, "DECI": -1, "CENTI": -2, "MILLI": -3, "MICRO": -6, "NANO": -9, "PICO": -12, "FEMTO": -15, "ATTO": -18}
PART_COUNT = {"NAMED_UNIT": 1, "SI_UNIT": 2, "CONVERSION_BASED_UNIT": 2,
              "LENGTH_UNIT": 0, "PLANE_ANGLE_UNIT": 0, "SOLID_ANGLE_UNIT": 0,
              "REPRESENTATION_CONTEXT": 2, "GEOMETRIC_REPRESENTATION_CONTEXT": 1,
              "GLOBAL_UNIT_ASSIGNED_CONTEXT": 1, "GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT": 1,
              "REPRESENTATION": 3, "MEASURE_WITH_UNIT": 2,
              "LENGTH_MEASURE_WITH_UNIT": 0, "PLANE_ANGLE_MEASURE_WITH_UNIT": 0,
              "DIMENSIONAL_EXPONENTS": 7}


def bounded_read(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_BYTES:
        raise ValueError(f"not a bounded regular input: {path}")
    with path.open("rb") as stream:
        result = stream.read(MAX_BYTES + 1)
    if len(result) > MAX_BYTES:
        raise ValueError("input grew past byte limit")
    return result


class Parser:
    def __init__(self, raw):
        # Byte-preserving decoding: legacy author strings mix encodings. The
        # oracle never interprets these strings as identities or unit names.
        text = raw.decode("latin-1")
        self.tokens = []
        cursor = 0
        while cursor < len(text):
            match = TOKEN.match(text, cursor)
            if not match:
                # ISO-10303 prologue/epilogue contain hyphens outside DATA;
                # ignore these only before locating DATA, never within records.
                raise ValueError(f"unsupported STEP token at offset {cursor}")
            token = match.group()
            cursor = match.end()
            if not token.isspace() and not token.startswith("/*"):
                self.tokens.append(token)
                if len(self.tokens) > MAX_TOKENS:
                    raise ValueError("STEP token budget exceeded")
        self.pos = 0

    def pop(self):
        if self.pos >= len(self.tokens):
            raise ValueError("truncated STEP record")
        token = self.tokens[self.pos]
        self.pos += 1
        return token

    def expect(self, token):
        if self.pop() != token:
            raise ValueError(f"expected STEP delimiter {token}")

    def args(self, depth):
        if depth > MAX_DEPTH:
            raise ValueError("STEP aggregate depth exceeded")
        self.expect("(")
        args = []
        if self.tokens[self.pos] == ")":
            self.pos += 1
            return args
        while True:
            args.append(self.value(depth + 1))
            delimiter = self.pop()
            if delimiter == ")":
                return args
            if delimiter != ",":
                raise ValueError("unbalanced STEP aggregate")

    def value(self, depth):
        if depth > MAX_DEPTH:
            raise ValueError("STEP value depth exceeded")
        token = self.tokens[self.pos]
        if token == "(":
            return self.args(depth)
        token = self.pop()
        if token.startswith("#"):
            return ("ref", int(token[1:]))
        if token.startswith("'"):
            return ("string", token[1:-1].replace("''", "'"))
        if token.startswith('"'):
            return ("binary", token)
        if token.startswith(".") and token.endswith(".") and token[1].isalpha():
            return ("enum", token[1:-1].upper())
        if token in {"$", "*"}:
            return token
        if token[0].isalpha():
            return ("typed", token.upper(), self.args(depth))
        number = float(token) if any(c in token for c in ".Ee") else int(token)
        if not math.isfinite(number):
            raise ValueError("nonfinite STEP number")
        return number

    def entities(self):
        records = {}
        while self.pos < len(self.tokens) and self.tokens[self.pos] != "ENDSEC":
            label = self.pop()
            if not label.startswith("#"):
                raise ValueError("expected STEP entity reference")
            identity = int(label[1:])
            if identity in records or len(records) >= MAX_ENTITIES:
                raise ValueError("duplicate entity or entity budget exceeded")
            self.expect("=")
            complex_entity = self.tokens[self.pos] == "("
            parts = {}
            if complex_entity:
                self.pos += 1
                while self.tokens[self.pos] != ")":
                    name = self.pop().upper()
                    if name in parts:
                        raise ValueError("duplicate complex entity part")
                    parts[name] = self.args(0)
                self.pos += 1
            else:
                name = self.pop().upper()
                parts[name] = self.args(0)
            self.expect(";")
            records[identity] = (complex_entity, parts)
        self.expect("ENDSEC")
        self.expect(";")
        if self.pos != len(self.tokens):
            raise ValueError("unsupported data sections or trailing DATA tokens")
        return records


def parse(raw):
    # Section boundaries are found outside quoted strings/comments, not by a
    # model-wide SI-unit expression. Tokenize only the bounded DATA section.
    text = raw.decode("latin-1")
    marker = re.compile(r"'(?:''|[^'])*'|/\*.*?\*/|\b(DATA|ENDSEC)\s*;", re.S)
    start = end = None
    for match in marker.finditer(text):
        if match.group(1) == "DATA" and start is None:
            start = match.end()
        elif match.group(1) == "ENDSEC" and start is not None:
            end = match.end()
            break
    if start is None or end is None:
        raise ValueError("missing bounded DATA section")
    return Parser(text[start:end].encode("latin-1")).entities()


def ref(value):
    if not isinstance(value, tuple) or len(value) != 2 or value[0] != "ref":
        raise ValueError("unit/context link is not a reference")
    return value[1]


class Oracle:
    def __init__(self, entities):
        self.entities = entities
        self.work = 0
        self.opaque = Counter()

    def entity(self, identity):
        self.work += 1
        if self.work > MAX_WORK:
            raise ValueError("independent unit-graph work budget exceeded")
        if identity not in self.entities:
            raise ValueError(f"dangling STEP entity #{identity}")
        return self.entities[identity]

    def part(self, identity, name):
        complex_entity, parts = self.entity(identity)
        if complex_entity:
            args = parts.get(name)
            if args is None:
                raise ValueError(f"entity #{identity} lacks explicit {name}")
            if name not in PART_COUNT or len(args) != PART_COUNT[name]:
                raise ValueError(f"unsupported/wrong-size {name} partial")
            return args
        kind, args = next(iter(parts.items()))
        if name == "REPRESENTATION" and kind in REPRESENTATIONS:
            expected, offset, count = 3, 0, 3
        elif name == "NAMED_UNIT" and kind in {"NAMED_UNIT", "SI_UNIT", "CONVERSION_BASED_UNIT", "LENGTH_UNIT", "PLANE_ANGLE_UNIT", "SOLID_ANGLE_UNIT"}:
            expected, offset, count = (3 if kind in {"SI_UNIT", "CONVERSION_BASED_UNIT"} else 1), 0, 1
        elif name == "SI_UNIT" and kind == "SI_UNIT":
            expected, offset, count = 3, 1, 2
        elif name == "CONVERSION_BASED_UNIT" and kind == "CONVERSION_BASED_UNIT":
            expected, offset, count = 3, 1, 2
        elif name == "MEASURE_WITH_UNIT" and kind in {"MEASURE_WITH_UNIT", "LENGTH_MEASURE_WITH_UNIT", "PLANE_ANGLE_MEASURE_WITH_UNIT"}:
            expected, offset, count = 2, 0, 2
        elif name == "GEOMETRIC_REPRESENTATION_CONTEXT" and kind == name:
            expected, offset, count = 3, 2, 1
        elif name == "GLOBAL_UNIT_ASSIGNED_CONTEXT" and kind == name:
            expected, offset, count = 3, 2, 1
        elif name == "DIMENSIONAL_EXPONENTS" and kind == name:
            expected, offset, count = 7, 0, 7
        else:
            raise ValueError(f"unsupported unit/schema mapping {kind} as {name}")
        if len(args) != expected:
            raise ValueError(f"wrong argument count for {kind}")
        return args[offset:offset + count]

    def names(self, identity):
        return set(self.entity(identity)[1])

    def dimensions(self, identity, derived_allowed):
        dimensions = self.part(identity, "NAMED_UNIT")[0]
        if dimensions == "*" and derived_allowed:
            return
        exponents = self.part(ref(dimensions), "DIMENSIONAL_EXPONENTS")
        if exponents != [1, 0, 0, 0, 0, 0, 0]:
            raise ValueError("unit dimensions are not length")

    def length(self, identity, visiting=None, depth=0):
        visiting = set() if visiting is None else visiting
        if depth > 32 or identity in visiting:
            raise ValueError("conversion graph depth/cycle refusal")
        visiting.add(identity)
        names = self.names(identity)
        if names & {"PLANE_ANGLE_UNIT", "SOLID_ANGLE_UNIT"}:
            raise ValueError("angle unit cannot be length conversion base")
        if "SI_UNIT" in names:
            if "CONVERSION_BASED_UNIT" in names:
                raise ValueError("ambiguous unit definition")
            self.dimensions(identity, True)
            prefix, name = self.part(identity, "SI_UNIT")
            if name != ("enum", "METRE"):
                raise ValueError("length SI unit does not name METRE")
            if prefix == "$":
                value = 1.0
            elif isinstance(prefix, tuple) and prefix[0] == "enum" and prefix[1] in PREFIX_EXPONENTS:
                value = 10.0 ** PREFIX_EXPONENTS[prefix[1]]
            else:
                raise ValueError("unsupported SI prefix")
        elif "CONVERSION_BASED_UNIT" in names:
            if "LENGTH_UNIT" not in names:
                raise ValueError("conversion missing LENGTH_UNIT")
            self.dimensions(identity, False)
            _, measure = self.part(identity, "CONVERSION_BASED_UNIT")
            measure_id = ref(measure)
            if measure_id in visiting:
                raise ValueError("conversion measure cycle")
            visiting.add(measure_id)
            if "PLANE_ANGLE_MEASURE_WITH_UNIT" in self.names(measure_id):
                raise ValueError("conversion factor is angle measure")
            typed, base = self.part(measure_id, "MEASURE_WITH_UNIT")
            if not isinstance(typed, tuple) or len(typed) != 3 or typed[:2] != ("typed", "LENGTH_MEASURE") or len(typed[2]) != 1:
                raise ValueError("conversion factor lacks typed LENGTH_MEASURE")
            factor = typed[2][0]
            if not isinstance(factor, (int, float)) or not math.isfinite(factor) or factor <= 0:
                raise ValueError("invalid conversion factor")
            value = factor * self.length(ref(base), visiting, depth + 1)
            visiting.remove(measure_id)
        else:
            raise ValueError(f"unsupported length unit entity #{identity}: {sorted(names)}")
        visiting.remove(identity)
        if not math.isfinite(value) or value <= 0:
            raise ValueError("invalid metre scale")
        return value

    def rows(self):
        rows = []
        parametric_rows = []
        representation_count = 0
        for identity in sorted(self.entities):
            names = set(self.entities[identity][1])
            if not names & REPRESENTATIONS:
                # These are association entities, not REPRESENTATION subtypes.
                associations = {"SHAPE_DEFINITION_REPRESENTATION", "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION"}
                if any(name.endswith("_REPRESENTATION") and name not in associations for name in names):
                    raise ValueError(f"unsupported representation subtype #{identity}: {names}")
                continue
            representation_count += 1
            context_id = ref(self.part(identity, "REPRESENTATION")[2])
            rep_names = self.names(identity)
            context_names = self.names(context_id)
            if "PARAMETRIC_REPRESENTATION_CONTEXT" in context_names:
                if ("DEFINITIONAL_REPRESENTATION" not in rep_names
                        or "GEOMETRIC_REPRESENTATION_CONTEXT" not in context_names
                        or self.part(context_id, "GEOMETRIC_REPRESENTATION_CONTEXT")[0] != 2
                        or "GLOBAL_UNIT_ASSIGNED_CONTEXT" in context_names):
                    raise ValueError(f"unsupported parametric context profile #{identity}")
                parametric_rows.append((identity, context_id))
                continue
            if "GEOMETRIC_REPRESENTATION_CONTEXT" not in context_names:
                continue
            if self.part(context_id, "GEOMETRIC_REPRESENTATION_CONTEXT")[0] not in (2, 3):
                raise ValueError("unsupported coordinate dimensions")
            assigned = self.part(context_id, "GLOBAL_UNIT_ASSIGNED_CONTEXT")[0]
            if not isinstance(assigned, list):
                raise ValueError("assigned units must be aggregate")
            seen, length_ids = set(), []
            for value in assigned:
                unit_id = ref(value)
                if unit_id in seen:
                    raise ValueError("duplicate assigned unit")
                seen.add(unit_id)
                unit_names = self.names(unit_id)
                si_name = self.part(unit_id, "SI_UNIT")[1] if "SI_UNIT" in unit_names else None
                if "LENGTH_UNIT" in unit_names or si_name == ("enum", "METRE"):
                    length_ids.append(unit_id)
                elif unit_names & {"PLANE_ANGLE_UNIT", "SOLID_ANGLE_UNIT"} or si_name in {( "enum", "RADIAN"), ("enum", "STERADIAN")}:
                    pass
                else:
                    raise ValueError(f"unsupported assigned quantity #{unit_id}")
            if len(length_ids) != 1:
                raise ValueError("context requires exactly one length unit")
            unit_id = length_ids[0]
            rows.append((identity, context_id, unit_id, self.length(unit_id)))
        if not rows and not parametric_rows:
            raise ValueError("no explicitly resolved geometric representation")
        return representation_count, rows, parametric_rows


def digest_rows(rows):
    digest = hashlib.sha256()
    for representation, context, unit, scale in rows:
        digest.update(struct.pack("<QQQd", representation, context, unit, scale))
    return digest.hexdigest()


def digest_parametric_rows(rows):
    digest = hashlib.sha256()
    for representation, context in rows:
        digest.update(struct.pack("<QQ", representation, context))
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--models", type=Path, default=DEFAULT_MODELS)
    parser.add_argument("--source-receipt", type=Path, default=SOURCE_RECEIPT)
    parser.add_argument("--output", type=Path, default=ROOT / "target/github-delivery/step-length-unit-preflight")
    args = parser.parse_args()
    output = args.output.absolute().resolve()
    allowed = (ROOT / "target").resolve()
    if not output.is_relative_to(allowed) or output.exists():
        raise ValueError("receipt directory must be new and inside CLI target")
    output.mkdir(parents=True)
    original = json.loads(bounded_read(args.source_receipt))
    source_models = {item["model"]: item["sha256"] for item in original["models"]}
    models = []
    try:
        for number in range(17):
            path = args.models / f"{number}.step"
            raw = bounded_read(path)
            source_hash = hashlib.sha256(raw).hexdigest()
            if source_models.get(path.name) != source_hash:
                raise ValueError(f"pinned source receipt mismatch: {path.name}")
            rep_count, rows, parametric_rows = Oracle(parse(raw)).rows()
            expected_digest = digest_rows(rows)
            run = subprocess.run([str(args.binary.resolve()), "--units-only", str(path.resolve())], capture_output=True, timeout=60)
            if len(run.stdout) > 4 * 1024 * 1024 or len(run.stderr) > 1024 * 1024:
                raise ValueError("experimental subprocess output budget exceeded")
            if run.returncode != 2:
                raise AssertionError(f"expected deliberate nonadmission exit2, got {run.returncode}: {run.stderr[:500]!r}")
            probe = json.loads(run.stdout)
            report = probe["declaredRepresentationLengthUnits"]
            assert probe["sourceSha256"] == source_hash and probe["sourceBytes"] == len(raw)
            assert probe["admitted"] is False and probe["exportsProduced"] == []
            assert report["analysisComplete"] is True
            assert report["lengthUnitsResolved"] is (len(rows) > 0)
            assert report["failedRepresentations"] == 0
            assert report["representationsTotal"] == rep_count and report["representationsInspected"] == rep_count
            assert report["resolvedGeometricRepresentations"] == len(rows)
            assert report["resolvedContextRowsSha256"] == expected_digest, f"full-context digest differs: {path.name}"
            assert report["parametricRepresentationsClassified"] == len(parametric_rows)
            assert report["parametricRepresentationRowsSha256"] == digest_parametric_rows(parametric_rows)
            assert report["geometricContextCount"] == len({row[1] for row in rows})
            row_lookup = {row[0]: row for row in rows}
            for detail in report["records"]:
                if detail.get("resolved"):
                    row = row_lookup[detail["representationId"]]
                    assert (detail["contextId"], detail["lengthUnitId"], detail["metresPerUnit"]) == row[1:]
                elif detail.get("reason") == "surface_parameter_space_not_physical_length":
                    assert (detail["representationId"],detail["contextId"]) in parametric_rows
            (output / f"{number}-rust.json").write_text(json.dumps(probe, indent=2) + "\n", encoding="utf-8")
            model = {"model":path.name,"sourceSha256":source_hash,"sourceBytes":len(raw),"sourceHashMatchesPinnedReceipt":True,"exitCode":run.returncode,"representationsCompared":rep_count,"geometricRowsCompared":len(rows),"parametricRowsCompared":len(parametric_rows),"geometricContextsCompared":len({row[1] for row in rows}),"fullRowsSha256":expected_digest,"parametricRowsSha256":digest_parametric_rows(parametric_rows),"distinctMetresPerUnit":sorted({row[3] for row in rows}),"passed":True,"admitted":False}
            models.append(model)
            print(f"PASS {path.name}: {len(rows)} physical-unit rows and {len(parametric_rows)} parameter-space rows; {rep_count} total representations", flush=True)
        receipt={"schema":"tokito.step-unit-independent-e2e.v1","passed":True,"models":models,"sourceCommit":original["sourceCommit"],"sourceReceiptSha256":hashlib.sha256(bounded_read(args.source_receipt)).hexdigest(),"binarySha256":hashlib.sha256(bounded_read(args.binary)).hexdigest(),"oracleScope":"independent balanced STEP tokenizer and linked declared representation/context/unit schema subset; parametric 2D coordinates are classified without physical scale; opaque geometric entities not interpreted","digestFraming":"ascending physical representationId; struct.pack('<QQQd',representationId,contextId,lengthUnitId,metresPerUnit); parameter rows use struct.pack('<QQ',representationId,contextId)","geometryQualified":False,"assemblyPoseVerified":False,"admitted":False}
    except Exception as error:
        receipt={"schema":"tokito.step-unit-independent-e2e.v1","passed":False,"models":models,"errorType":type(error).__name__,"error":str(error),"admitted":False}
        (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        raise
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"All17 unit-graph comparisons passed; receipt: {output / 'receipt.json'}")


if __name__ == "__main__":
    main()
