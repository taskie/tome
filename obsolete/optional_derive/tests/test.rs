#[macro_use]
extern crate optional_derive;

#[derive(Debug, PartialEq, Optional)]
struct S1 {
    pub x: i32,
    pub y: Option<i32>,
}

#[derive(Default, Debug, PartialEq, Optional)]
#[optional(name = "OptionalS2", derive = "Default, Debug, PartialEq")]
struct S2<'a> {
    #[optional(name = "z")]
    pub x: i32,
    #[optional(skip = true)]
    pub y: Option<i32>,
    #[optional(name = "x", required = true)]
    pub z: bool,
    pub w: &'a str,
}

#[test]
fn test_simple() {
    let _s1 = S1 { x: 42, y: Some(42) };
    let _s1_opt = S1Opt { x: Some(42), y: Some(Some(42)) };
}

#[test]
fn test_complex() {
    let _s2 = S2 { x: 42, y: Some(42), z: true, w: "hello" };
    let s2_opt = OptionalS2 { x: true, z: Some(42), w: Some("hello") };
    format!("{:?}", s2_opt);
    assert_eq!(OptionalS2 { x: true, z: Some(42), w: Some("hello") }, s2_opt);
}

#[test]
fn test_complex_default() {
    assert_eq!(S2 { x: 0, y: None, z: false, w: "" }, Default::default());
    assert_eq!(OptionalS2 { x: false, z: None, w: None }, Default::default());
}

#[test]
fn test_complex_from() {
    let s2 = S2 { x: 42, y: Some(42), z: true, w: "hello" };
    assert_eq!(OptionalS2 { x: true, z: Some(42), w: Some("hello") }, OptionalS2::from(s2));
}
