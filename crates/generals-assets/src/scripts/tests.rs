use super::*;

fn scalar(kind: i32, text: &str) -> Vec<u8> {
    let mut bytes = kind.to_le_bytes().to_vec();
    bytes.extend(42_i32.to_le_bytes());
    bytes.extend(1.25_f32.to_le_bytes());
    bytes.extend(u16::try_from(text.len()).unwrap().to_le_bytes());
    bytes.extend(text.as_bytes());
    bytes
}

#[test]
fn unknown_calls_keep_parameter_slots_and_coordinate_alignment() {
    let symbols = BTreeMap::from([(1, String::from("FutureOperation"))]);
    let mut bytes = 999_i32.to_le_bytes().to_vec();
    bytes.extend(259_u32.to_le_bytes());
    bytes.extend(3_i32.to_le_bytes());
    bytes.extend(scalar(15, "FundamentalistInfantryRebel"));
    bytes.extend(16_i32.to_le_bytes());
    for position in [12.0_f32, -3.0, 7.0] {
        bytes.extend(position.to_le_bytes());
    }
    bytes.extend(scalar(32, "Upgrade_GLARebelCaptureBuilding"));
    let call = read_call(
        &Chunk {
            name: "ScriptAction",
            version: 2,
            bytes: &bytes,
        },
        &symbols,
        2,
    )
    .unwrap();
    assert_eq!(call.id, 999);
    assert_eq!(call.name.as_deref(), Some("FutureOperation"));
    assert_eq!(call.parameters[0].text, "GLAInfantryRebel");
    assert_eq!(call.parameters[0].integer, 42);
    assert!((call.parameters[0].real - 1.25).abs() < f32::EPSILON);
    assert_eq!(call.parameters[1].position, Some([12.0, -3.0, 7.0]));
    assert_eq!(call.parameters[2].text, "Upgrade_InfantryCaptureBuilding");
    bytes.pop();
    assert!(matches!(
        read_call(
            &Chunk {
                name: "ScriptAction",
                version: 2,
                bytes: &bytes
            },
            &symbols,
            2
        ),
        Err(AssetError::Invalid("truncated input"))
    ));
}

#[test]
fn malformed_name_keys_and_parameter_counts_are_rejected() {
    let symbols = BTreeMap::from([(1, String::from("True"))]);
    for (key, count, expected) in [
        (258_u32, 0_i32, "script name key has wrong type"),
        (515, 0, "unknown script name key"),
        (259, -1, "invalid script parameter count"),
        (259, 13, "invalid script parameter count"),
    ] {
        let mut bytes = 0_i32.to_le_bytes().to_vec();
        bytes.extend(key.to_le_bytes());
        bytes.extend(count.to_le_bytes());
        assert!(
            matches!(read_call(&Chunk { name: "Condition", version: 4, bytes: &bytes }, &symbols, 4), Err(AssetError::Invalid(reason)) if reason == expected)
        );
    }
}
