const LOWER_HEX_CHARS: [char; 16] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f'];

pub fn to_hex_string(bs: &[u8]) -> String {
    let mut s = String::with_capacity(bs.len() * 2);
    for b in bs.iter() {
        s.push(LOWER_HEX_CHARS[((b & 0b11110000) >> 4) as usize]);
        s.push(LOWER_HEX_CHARS[(b & 0b00001111) as usize]);
    }
    s
}

pub fn from_hex_string(s: &str) -> Option<Vec<u8>> {
    let mut bs = Vec::with_capacity(s.len() / 2);
    let mut iter = s.chars();
    loop {
        let c1 = iter.next();
        let c2 = iter.next();
        match (c1, c2) {
            (Some(c1), Some(c2)) => {
                let b1 = match c1 {
                    '0'..='9' => c1 as u8 - '0' as u8,
                    'a'..='f' => c1 as u8 - 'a' as u8 + 10,
                    'A'..='F' => c1 as u8 - 'A' as u8 + 10,
                    _ => return None,
                };
                let b2 = match c2 {
                    '0'..='9' => c2 as u8 - '0' as u8,
                    'a'..='f' => c2 as u8 - 'a' as u8 + 10,
                    'A'..='F' => c2 as u8 - 'A' as u8 + 10,
                    _ => return None,
                };
                bs.push((b1 << 4) | b2);
            }
            (None, None) => break,
            _ => return None,
        }
    }
    Some(bs)
}

#[test]
fn test_to_hex_string() {
    assert_eq!(to_hex_string(&[]), "");
    assert_eq!(to_hex_string(&[0x00, 0xff]), "00ff");
    assert_eq!(to_hex_string(&[0xfe, 0xdc, 0xba, 0x98, 0x76]), "fedcba9876");
}

#[test]
fn test_from_hex_string() {
    assert_eq!(from_hex_string(""), Some(vec![]));
    assert_eq!(from_hex_string("00ff"), Some(vec![0x00, 0xff]));
    assert_eq!(from_hex_string("fedcba9876"), Some(vec![0xfe, 0xdc, 0xba, 0x98, 0x76]));
    assert_eq!(from_hex_string("FEDCBA9876"), Some(vec![0xfe, 0xdc, 0xba, 0x98, 0x76]));
    assert_eq!(from_hex_string("fedcba987"), None);
    assert_eq!(from_hex_string("x"), None);
}
