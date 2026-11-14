pub mod limits;

pub const VERSION: &str = match option_env!("QUORUM_VERSION") {
    Some(v) => v,
    None => "dev",
};
