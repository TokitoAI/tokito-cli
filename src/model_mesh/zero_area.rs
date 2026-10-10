//! Explicit byte-preserving removal of exactly zero-area decimal source faces.
use super::{
    EngineError, MeshLimits, ModelUnits, fail, palette_derivation, parse_mesh_internal,
    parse_vrml_mesh, tokenize,
};
use serde_json::json;

fn decimal(token: &str) -> Result<(i128, i32), EngineError> {
    let (mantissa, exponent) = match token.find(['e', 'E']) {
        Some(i) => (
            &token[..i],
            token[i + 1..]
                .parse::<i32>()
                .map_err(|_| fail("invalid exact decimal exponent"))?,
        ),
        None => (token, 0),
    };
    if !(-18..=18).contains(&exponent) {
        return Err(fail("exact decimal exponent budget exceeded"));
    }
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.trim_start_matches(['-', '+']);
    let mut digits = String::new();
    let mut fractional = 0i32;
    let mut dot = false;
    for c in unsigned.chars() {
        if c == '.' && !dot {
            dot = true;
        } else if c.is_ascii_digit() {
            digits.push(c);
            if dot {
                fractional += 1;
            }
        } else {
            return Err(fail("unsupported exact decimal token"));
        }
    }
    if digits.is_empty() || digits.len() > 18 {
        return Err(fail("exact decimal digit budget exceeded"));
    }
    let mut value = digits
        .parse::<i128>()
        .map_err(|_| fail("invalid exact decimal"))?;
    if negative {
        value = -value;
    }
    let mut scale = exponent - fractional;
    if value == 0 {
        return Ok((0, 0));
    }
    while value % 10 == 0 {
        value /= 10;
        scale += 1;
    }
    if !(-18..=18).contains(&scale) {
        return Err(fail("exact decimal scale budget exceeded"));
    }
    Ok((value, scale))
}

fn collinear(points: [&[&str]; 3]) -> Result<bool, EngineError> {
    let mut values = Vec::with_capacity(9);
    for point in points {
        for token in point {
            values.push(decimal(token)?);
        }
    }
    let scale = values
        .iter()
        .filter(|(v, _)| *v != 0)
        .map(|(_, s)| *s)
        .min()
        .unwrap_or(0);
    let mut integers = [0i128; 9];
    for (i, (value, exponent)) in values.iter().enumerate() {
        if *value != 0 {
            let power = u32::try_from(exponent - scale).map_err(|_| fail("invalid exact scale"))?;
            integers[i] = 10i128
                .checked_pow(power)
                .and_then(|m| value.checked_mul(m))
                .ok_or_else(|| fail("exact integer scaling budget exceeded"))?;
        }
    }
    let mut u = [0i128; 3];
    let mut v = [0i128; 3];
    for i in 0..3 {
        u[i] = integers[i + 3]
            .checked_sub(integers[i])
            .ok_or_else(|| fail("exact difference overflow"))?;
        v[i] = integers[i + 6]
            .checked_sub(integers[i])
            .ok_or_else(|| fail("exact difference overflow"))?;
    }
    for (a, b) in [(1, 2), (2, 0), (0, 1)] {
        let cross = u[a]
            .checked_mul(v[b])
            .and_then(|left| {
                u[b].checked_mul(v[a])
                    .and_then(|right| left.checked_sub(right))
            })
            .ok_or_else(|| fail("exact cross-product budget exceeded"))?;
        if cross != 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// No epsilon, rounding or coordinate alteration. Exact-arithmetic overflow
/// refuses rather than classifying a face. The final strict parser is mandatory.
pub fn derive_without_zero_area_faces(
    input: &[u8],
    units: ModelUnits,
    remove_unused_palette: bool,
) -> Result<(Vec<u8>, serde_json::Value), EngineError> {
    if input.len() > MeshLimits::default().max_bytes {
        return Err(fail("byte budget exceeded"));
    }
    let (face_source, palette) = if remove_unused_palette {
        palette_derivation(input)?
    } else {
        (input.to_vec(), json!(null))
    };
    // Validate the entire supported grammar, materials and indices before any
    // deletion; only normals are deferred. This private mesh never escapes.
    parse_mesh_internal(&face_source, units, MeshLimits::default(), true)?;
    let text = std::str::from_utf8(&face_source).map_err(|_| fail("input is not UTF-8"))?;
    let tokens = tokenize(text, MeshLimits::default())?;
    let offset = |t: &str| t.as_ptr() as usize - text.as_ptr() as usize;
    let mut cursor = 0;
    let mut ordinal = 0;
    let mut spans = Vec::new();
    let mut faces = Vec::new();
    let mut emptied = Vec::new();
    while cursor < tokens.len() {
        let start = cursor;
        cursor += 2;
        let mut depth = 1;
        while depth > 0 {
            match tokens[cursor] {
                "{" => depth += 1,
                "}" => depth -= 1,
                _ => (),
            }
            cursor += 1;
        }
        let shape = &tokens[start..cursor];
        // A Material DEF/USE identifier may itself be named point or
        // coordIndex. Only validated field + list delimiters identify geometry.
        let point_at = shape.windows(2).position(|w| w == ["point", "["]);
        let index_at = shape.windows(2).position(|w| w == ["coordIndex", "["]);
        if let (Some(point_at), Some(index_at)) = (point_at, index_at) {
            let points_end = shape[point_at + 2..]
                .iter()
                .position(|t| *t == "]")
                .unwrap()
                + point_at
                + 2;
            let points = &shape[point_at + 2..points_end];
            if points.len() % 3 != 0 {
                return Err(fail("incomplete exact coordinate triple"));
            }
            let end = shape[index_at + 2..]
                .iter()
                .position(|t| *t == "]")
                .unwrap()
                + index_at
                + 2;
            let indices = &shape[index_at + 2..end];
            let mut local_spans = Vec::new();
            let mut retained = 0;
            for (face_ordinal, face) in indices.chunks_exact(4).enumerate() {
                let ids = [
                    face[0].parse::<usize>().unwrap(),
                    face[1].parse::<usize>().unwrap(),
                    face[2].parse::<usize>().unwrap(),
                ];
                if collinear(ids.map(|i| &points[i * 3..i * 3 + 3]))? {
                    let begin = offset(face[0]);
                    let finish = offset(face[3]) + face[3].len();
                    local_spans.push((begin, finish));
                    faces.push(json!({"rootShapeOrdinalInFaceSource":ordinal,"faceOrdinal":face_ordinal,"indices":ids,"startByte":begin,"endByteExclusive":finish,"removedBytesSha256":crate::sha256_hex(&face_source[begin..finish]),"reason":"exact_decimal_zero_cross_product"}));
                } else {
                    retained += 1;
                }
            }
            if retained == 0 {
                if shape.contains(&"DEF") {
                    return Err(fail(
                        "empty geometry Shape contains a definition; removal refused",
                    ));
                }
                let begin = offset(shape[0]);
                let finish = offset(shape.last().unwrap()) + 1;
                spans.push((begin, finish));
                emptied.push(json!({"rootShapeOrdinalInFaceSource":ordinal,"startByte":begin,"endByteExclusive":finish,"removedBytesSha256":crate::sha256_hex(&face_source[begin..finish])}));
            } else {
                spans.extend(local_spans);
            }
        }
        ordinal += 1;
    }
    if faces.is_empty() {
        return Err(fail("no proven zero-area source faces"));
    }
    spans.sort_unstable();
    let mut derived = Vec::new();
    let mut begin = 0;
    for (start, end) in &spans {
        derived.extend_from_slice(&face_source[begin..*start]);
        begin = *end;
    }
    derived.extend_from_slice(&face_source[begin..]);
    let mesh = parse_vrml_mesh(&derived, units, MeshLimits::default())?;
    let report = json!({"schema":"tokito.vrml-zero-area-derivation.v1","sourceSha256":crate::sha256_hex(input),"derivedSha256":crate::sha256_hex(&derived),"faceSourceSha256":crate::sha256_hex(&face_source),"faceSpanCoordinateSystem":"byte offsets and zero-based root Shape ordinals in faceSourceSha256, after optional palette derivation","paletteDerivation":palette,"removedZeroAreaFaces":faces,"removedEmptyGeometryShapes":emptied,"removedByteSpans":spans,"retainedBytesUnchanged":true,"coordinateValuesModified":false,"meshTopologyModified":true,"surfaceFidelityVerified":false,"physicalFitVerified":false,"placementVerified":false,"cadSolidVerified":false,"derivedMeshParts":mesh.parts.len(),"derivedTriangles":mesh.parts.iter().map(|p|p.triangles.len()).sum::<usize>()});
    Ok((derived, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proves_decimal_collinearity_without_unit_rounding_or_epsilon() {
        assert!(
            collinear([
                &["-.264", "-.185", ".195"],
                &["-.270", "-.185", ".197"],
                &["-.261", "-.185", ".194"]
            ])
            .unwrap()
        );
        assert!(!collinear([&["0", "0", "0"], &["1", "0", "0"], &["0", "1e-18", "0"]]).unwrap());
        assert!(decimal("1e-100").is_err());
        assert!(
            collinear([
                &["1e18", "0", "0"],
                &["0", "1e18", "0"],
                &["1e-18", "0", "1e18"]
            ])
            .is_err()
        );
    }
    #[test]
    fn retains_nonzero_bytes_removes_empty_shapes_and_refuses_unsupported_fields() {
        let input=b"#VRML V2.0 utf8\nShape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 0 0,0 1 0,2 0 0] } coordIndex [0,1,3,-1,0,1,2,-1] } }\nShape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 0 0,2 0 0] } coordIndex [0,1,2,-1] } }";
        let (bytes, report) =
            derive_without_zero_area_faces(input, ModelUnits::Millimetres, false).unwrap();
        assert_eq!(report["derivedTriangles"], 1);
        assert_eq!(report["removedZeroAreaFaces"].as_array().unwrap().len(), 2);
        assert_eq!(
            report["removedEmptyGeometryShapes"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(std::str::from_utf8(&bytes).unwrap().contains("0,1,2,-1"));
        let mut reconstructed = Vec::new();
        let mut begin = 0;
        for span in report["removedByteSpans"].as_array().unwrap() {
            let start = span[0].as_u64().unwrap() as usize;
            reconstructed.extend_from_slice(&input[begin..start]);
            begin = span[1].as_u64().unwrap() as usize;
        }
        reconstructed.extend_from_slice(&input[begin..]);
        assert_eq!(bytes, reconstructed);
        let unsupported = String::from_utf8(input.to_vec())
            .unwrap()
            .replace("IndexedFaceSet {", "IndexedFaceSet { convex TRUE");
        assert!(
            derive_without_zero_area_faces(unsupported.as_bytes(), ModelUnits::Millimetres, false)
                .is_err()
        );
        assert!(parse_vrml_mesh(input, ModelUnits::Millimetres, MeshLimits::default()).is_err());
        let invalid_index = String::from_utf8(input.to_vec())
            .unwrap()
            .replace("0,1,3,-1", "0,1,99,-1");
        assert!(
            derive_without_zero_area_faces(
                invalid_index.as_bytes(),
                ModelUnits::Millimetres,
                false
            )
            .is_err()
        );
        let empty_definition = String::from_utf8(input.to_vec()).unwrap().replace("Shape { geometry", "Shape { appearance Appearance { material DEF shared Material { diffuseColor .2 .3 .4 } } geometry");
        assert!(
            derive_without_zero_area_faces(
                empty_definition.as_bytes(),
                ModelUnits::Millimetres,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn material_identifiers_cannot_shadow_geometry_fields() {
        for name in ["point", "coordIndex"] {
            let input = format!(
                "#VRML V2.0 utf8\nShape {{ appearance Appearance {{ material DEF {name} Material {{ diffuseColor 0.2 0.3 0.4 }} }} geometry IndexedFaceSet {{ coord Coordinate {{ point [0 0 0,1 0 0,0 1 0,2 0 0] }} coordIndex [0,1,3,-1,0,1,2,-1] }} }}\nShape {{ appearance Appearance {{ material USE {name} }} geometry IndexedFaceSet {{ coord Coordinate {{ point [0 0 0,1 0 0,0 1 0] }} coordIndex [0,1,2,-1] }} }}"
            );
            let (_, report) =
                derive_without_zero_area_faces(input.as_bytes(), ModelUnits::Millimetres, false)
                    .unwrap();
            assert_eq!(report["derivedTriangles"], 2);
            assert_eq!(report["removedZeroAreaFaces"].as_array().unwrap().len(), 1);
        }
    }
}
