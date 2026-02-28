pub const DEFAULT_WORKSPACE_NAME: &'static str = "default";
pub const DEFAULT_GROUP_NAME: &'static str = "default";
pub const META_GROUP_NAME: &'static str = "__meta";
pub const ATTR_GROUP_NAME: &'static str = "__attr";

#[derive(Clone, Copy, Debug)]
pub enum Status {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Clone, Copy, Debug)]
pub enum GroupType {
    Local = 0,
    Remote = 1,
    Meta = 2,
    Attr = 3,
}

#[derive(Clone, Copy, Debug)]
pub enum ContentType {
    Unknown = 0,
    Json = 1,
    Text = 2,
}
