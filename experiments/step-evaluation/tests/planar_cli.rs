//! The source-native face diagnostic must never become an export success path.
use serde_json::Value;
use std::{fs, process::Command};

fn fixture() -> String {
    let mut data = String::new();
    let points = [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]];
    for (i, p) in points.iter().enumerate() {
        data += &format!(
            "#{}=CARTESIAN_POINT('',({},{},{}));#{}=VERTEX_POINT('',#{});",
            i + 1,
            p[0],
            p[1],
            p[2],
            i + 11,
            i + 1
        );
    }
    for i in 0..4 {
        let end = (i + 1) % 4;
        let direction: [i32; 3] = std::array::from_fn(|axis| points[end][axis] - points[i][axis]);
        data += &format!("#{}=DIRECTION('',({},{},{}));#{}=VECTOR('',#{},1.);#{}=LINE('',#{},#{});#{}=EDGE_CURVE('',#{},#{},#{},.T.);#{}=ORIENTED_EDGE('',*,*,#{},.T.);",21+i,direction[0],direction[1],direction[2],31+i,21+i,41+i,i+1,31+i,51+i,i+11,end+11,41+i,61+i,51+i);
    }
    data += "#25=DIRECTION('',(0.,0.,1.));#26=DIRECTION('',(1.,0.,0.));#71=EDGE_LOOP('',(#61,#62,#63,#64));#72=FACE_OUTER_BOUND('',#71,.T.);#73=AXIS2_PLACEMENT_3D('',#1,#25,#26);#74=PLANE('',#73);#75=ADVANCED_FACE('',(#72),#74,.T.);";
    format!("ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;{data}ENDSEC;END-ISO-10303-21;")
}

#[test]
fn actual_subprocess_reports_face_and_refuses_invalid_requests() {
    let root = std::env::temp_dir().join(format!(
        "tokito-planar-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let source = root.join("source.step");
    let original = fixture().replace("ENDSEC;END-ISO-10303-21;", "#80=MANIFOLD_SOLID_BREP('',#81);#81=CLOSED_SHELL('',(#75));#82=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#80),#83);#83=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#84)) REPRESENTATION_CONTEXT('',''));#84=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));ENDSEC;END-ISO-10303-21;");
    fs::write(&source, &original).unwrap();
    for (flags, success) in [
        (
            vec![
                "--planar-face",
                "75",
                "--source-tolerance",
                "1e-9",
                "--source-representation",
                "82",
            ],
            true,
        ),
        (
            vec![
                "--planar-face",
                "75",
                "--source-tolerance",
                "1e-9",
                "--source-representation",
                "80",
            ],
            false,
        ),
        (
            vec![
                "--planar-face",
                "75",
                "--source-tolerance",
                "1e-9",
                "--source-representation",
                "0",
            ],
            false,
        ),
        (
            vec![
                "--planar-face",
                "75",
                "--source-tolerance",
                "1e-9",
                "--source-representation",
                "invalid",
            ],
            false,
        ),
        (
            vec!["--planar-face", "75", "--source-tolerance", "1e-9"],
            true,
        ),
        (
            vec!["--planar-face", "75", "--source-tolerance", "0"],
            false,
        ),
        (
            vec!["--planar-face", "75", "--source-tolerance", "NaN"],
            false,
        ),
        (
            vec!["--planar-face", "999", "--source-tolerance", "1e-9"],
            false,
        ),
        (
            vec!["--planar-face", "75", "--source-tolerance", "1e-9", "extra"],
            false,
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_tokito-step-evaluation"))
            .args(flags)
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stderr.is_empty());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["admitted"], false);
        assert_eq!(report["exportsProduced"], serde_json::json!([]));
        assert_eq!(report.get("error").is_none(), success);
        if success {
            assert_eq!(report["triangleCount"], 2);
            assert_eq!(report["boundarySegmentsPreserved"], 4);
            assert_eq!(report["sourceLengthUnitsResolved"], false);
            assert_eq!(report["sourceToModelFrameVerified"], false);
            if let Some(selected) = report.get("selectedSourceRepresentation") {
                assert_eq!(selected["selectedRepresentationContainsFace"], true);
                assert_eq!(
                    selected["selectedRepresentationLengthUnits"]["metresPerUnit"],
                    0.001
                );
                assert_eq!(selected["sourceToModelFrameVerified"], false);
            }
        }
        assert_eq!(fs::read_to_string(&source).unwrap(), original);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }
    fs::remove_file(source).unwrap();
    fs::remove_dir(root).unwrap();
}
