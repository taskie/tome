pub const DEFAULT_WORKSPACE_NAME: &'static str = "default";
pub const DEFAULT_GROUP_NAME: &'static str = "default";
pub const META_GROUP_NAME: &'static str = "__meta";
pub const ATTR_GROUP_NAME: &'static str = "__attr";

#[derive(Clone, Copy, Debug)]
pub enum Status {
    DISABLED = 0,
    ENABLED = 1,
}

#[derive(Clone, Copy, Debug)]
pub enum GroupType {
    LOCAL = 0,
    REMOTE = 1,
    META = 2,
    ATTR = 3,
}

#[derive(Clone, Copy, Debug)]
pub enum ContentType {
    Unknown = 0,
    Json = 1,
    Text = 2,
}
