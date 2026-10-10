//! Source topology presence audit, never a tessellation or solid-validity certificate.
use oxideav_ifc::StepFile;
use oxideav_step::schema::Entity;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_WORK: usize = 100_000;
const MAX_DEPTH: usize = 32;
const MAX_DETAILS: usize = 128;

pub struct Audit<'a> {
    step: &'a StepFile,
    work: usize,
}
impl<'a> Audit<'a> {
    pub fn new(step: &'a StepFile) -> Self {
        Self { step, work: 0 }
    }
    fn walk(
        &mut self,
        id: u64,
        depth: usize,
        active: &mut BTreeSet<u64>,
        faces: &mut BTreeSet<u64>,
    ) -> Result<(), String> {
        self.work += 1;
        if self.work > MAX_WORK || depth > MAX_DEPTH {
            return Err("source_topology_budget_exceeded".into());
        }
        if !active.insert(id) {
            return Err(format!("source_topology_cycle:{id}"));
        }
        let inst = self
            .step
            .get(id)
            .ok_or_else(|| format!("dangling_source_topology:{id}"))?;
        let entity = Entity { inst };
        crate::unit_preflight::shape(inst)?;
        let mut children = Vec::new();
        if entity.is_a("ORIENTED_CLOSED_SHELL") {
            children.push(
                entity
                    .reference("closed_shell_element")
                    .ok_or("missing_oriented_shell_element")?,
            );
        } else if entity.is_a("ORIENTED_FACE") {
            children.push(
                entity
                    .reference("face_element")
                    .ok_or("missing_oriented_face_element")?,
            );
        } else if entity.is_a("MANIFOLD_SOLID_BREP") {
            children.push(entity.reference("outer").ok_or("missing_outer_shell")?);
            if entity.is_a("BREP_WITH_VOIDS") {
                for value in entity.list("voids").ok_or("missing_void_shell_list")? {
                    children.push(value.as_reference().ok_or("void_shell_is_not_reference")?);
                }
            }
        } else if entity.is_a("CLOSED_SHELL") || entity.is_a("OPEN_SHELL") {
            for value in entity.list("cfs_faces").ok_or("missing_shell_faces")? {
                children.push(value.as_reference().ok_or("shell_face_is_not_reference")?);
            }
            if children.is_empty() {
                return Err("empty_source_shell".into());
            }
        } else if entity.is_a("FACE_SURFACE") {
            faces.insert(id);
        } else {
            return Err(format!("unsupported_source_topology:{id}"));
        }
        if children.iter().copied().collect::<BTreeSet<_>>().len() != children.len() {
            return Err(format!("duplicate_shell_or_face_reference:{id}"));
        }
        for child in children {
            self.walk(child, depth + 1, active, faces)?;
        }
        active.remove(&id);
        Ok(())
    }
    pub fn compare(&mut self, item: u64, triangles: usize, provenance: &[u64]) -> Value {
        let mut expected = BTreeSet::new();
        let traversal = self.walk(item, 0, &mut BTreeSet::new(), &mut expected);
        let actual: BTreeSet<_> = provenance.iter().copied().filter(|id| *id != 0).collect();
        let missing: Vec<_> = expected.difference(&actual).copied().collect();
        let unexpected: Vec<_> = actual.difference(&expected).copied().collect();
        let unassigned = provenance.iter().filter(|id| **id == 0).count();
        let presence = traversal.is_ok()
            && !expected.is_empty()
            && missing.is_empty()
            && unexpected.is_empty()
            && unassigned == 0
            && provenance.len() == triangles;
        let mut digest = Sha256::new();
        for id in &expected {
            digest.update(id.to_le_bytes());
        }
        json!({
            "representationItemId":item,"sourceTraversalComplete":traversal.is_ok(),
            "diagnostic":traversal.err(),"expectedSourceFaces":expected.len(),
            "emittedDistinctSourceFaces":actual.len(),"missingSourceFaceCount":missing.len(),
            "unexpectedSourceFaceCount":unexpected.len(),"unassignedTriangleEntries":unassigned,
            "triangleProvenanceCountMatches":provenance.len()==triangles,
            "missingSourceFaceIds":missing.iter().take(MAX_DETAILS).collect::<Vec<_>>(),
            "unexpectedSourceFaceIds":unexpected.iter().take(MAX_DETAILS).collect::<Vec<_>>(),
            "detailsTruncated":missing.len()>MAX_DETAILS || unexpected.len()>MAX_DETAILS,
            "expectedSourceFaceIdsSha256":format!("{:x}",digest.finalize()),
            "digestFraming":"ascending unique source face IDs, little-endian u64, no header",
            "sourceFacePresenceMatches":presence,"workSteps":self.work,"maxWorkSteps":MAX_WORK,
            "scope":"shell-face membership reachable from this emitted shape item; face bounds, surface geometry and all source representation items unverified",
            "geometryQualified":false,"closedSolidVerified":false,"assemblyPoseVerified":false
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn step(data: &str) -> StepFile {
        oxideav_ifc::parse_step(format!("ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;{data}ENDSEC;END-ISO-10303-21;").as_bytes()).unwrap()
    }
    fn source() -> StepFile {
        step("#1=BREP_WITH_VOIDS('',#2,(#3));#2=CLOSED_SHELL('',(#5));#3=ORIENTED_CLOSED_SHELL('',*,#4,.F.);#4=CLOSED_SHELL('',(#6));#5=ADVANCED_FACE('',(),#9,.T.);#6=ADVANCED_FACE('',(),#9,.F.);")
    }
    #[test]
    fn void_faces_required_even_when_provenance_count_matches() {
        let input = source();
        let mut audit = Audit::new(&input);
        let missing = audit.compare(1, 2, &[5, 5]);
        assert_eq!(missing["expectedSourceFaces"], 2);
        assert_eq!(missing["missingSourceFaceIds"], json!([6]));
        assert_eq!(missing["sourceFacePresenceMatches"], false);
        let matched = audit.compare(1, 3, &[5, 6, 6]);
        assert_eq!(matched["sourceFacePresenceMatches"], true);
        assert_eq!(matched["geometryQualified"], false);
    }
    #[test]
    fn unexpected_unassigned_and_count_mismatch_refuse() {
        let input = source();
        let mut audit = Audit::new(&input);
        for (triangles, faces) in [(3, vec![5, 6, 99]), (3, vec![5, 6, 0]), (3, vec![5, 6])] {
            assert_eq!(
                audit.compare(1, triangles, &faces)["sourceFacePresenceMatches"],
                false
            );
        }
    }
    #[test]
    fn cyclic_dangling_unsupported_and_budget_refuse() {
        let input=step("#1=ORIENTED_CLOSED_SHELL('',*,#1,.T.);#2=CLOSED_SHELL('',(#99));#3=CARTESIAN_POINT('',(0.,0.,0.));");
        let mut audit = Audit::new(&input);
        for id in [1, 2, 3] {
            assert_eq!(audit.compare(id, 0, &[])["sourceTraversalComplete"], false);
        }
        audit.work = MAX_WORK;
        assert_eq!(
            audit.compare(3, 0, &[])["diagnostic"],
            "source_topology_budget_exceeded"
        );
    }
    #[test]
    fn malformed_arity_and_duplicate_set_members_refuse() {
        for data in [
            "#1=MANIFOLD_SOLID_BREP('',#2,'extra');#2=CLOSED_SHELL('',(#5));#5=ADVANCED_FACE('',(),#9,.T.);",
            "#1=MANIFOLD_SOLID_BREP('',#2);#2=CLOSED_SHELL('',(#5,#5));#5=ADVANCED_FACE('',(),#9,.T.);",
        ] {
            let input = step(data);
            assert_eq!(Audit::new(&input).compare(1, 1, &[5])["sourceTraversalComplete"], false);
        }
    }
}
