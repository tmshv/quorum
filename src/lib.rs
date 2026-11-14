pub mod limits;
pub mod rate;

pub const VERSION: &str = match option_env!("QUORUM_VERSION") {
    Some(v) => v,
    None => "dev",
};
