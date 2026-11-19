//! Java's EUC-JP and Big5 encoders (`charsets.json`, identical in the three JDKs): every BMP char, the
//! supplementary range, the names, and whole-string vectors.

mod common;

use common::*;
use java_compat::charset::Charset;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn encoders_as_recorded() {
    for provider in ["full", "javabase", "jdk21"] {
        for r in fixture(provider, "charsets").as_array().unwrap() {
            let name = r["name"].as_str().unwrap();
            let cs = Charset::for_name(name).unwrap();
            assert_eq!(cs.name(), r["canonical"].as_str().unwrap());
            for a in r["aliases"].as_array().unwrap() {
                let a = a.as_str().unwrap();
                assert_eq!(Charset::for_name(a), Some(cs), "{a}");
                assert_eq!(Charset::for_name(&a.to_ascii_uppercase()), Some(cs), "{a}");
            }
            let mapped: std::collections::HashMap<u32, &str> =
                r["map"].as_array().unwrap().iter().map(|e| (e[0].as_u64().unwrap() as u32, e[1].as_str().unwrap())).collect();
            for cp in 0..=0xFFFFu32 {
                let Some(c) = char::from_u32(cp) else { continue };
                let want = match mapped.get(&cp) {
                    Some(h) => h.to_string(),
                    None if cp < 0x80 => format!("{cp:02x}"),
                    None => "3f".into(),
                };
                assert_eq!(hex(&cs.encode(&c.to_string())), want, "{name} U+{cp:04X}");
            }
            for cp in (0x10000..=0x10FFFFu32).step_by(97) {
                assert_eq!(cs.encode(&char::from_u32(cp).unwrap().to_string()), b"?", "{name} U+{cp:X}");
            }
            for v in r["vectors"].as_array().unwrap() {
                let text = v[0].as_str().unwrap();
                assert_eq!(hex(&cs.encode(text)), v[1].as_str().unwrap(), "{name} {text:?}");
            }
        }
    }
    assert!(Charset::for_name("EUC-KR").is_none() && Charset::for_name("Big5-HKSCS").is_none());
}
