pub mod limits;
pub mod rate;
pub mod room;

pub const VERSION: &str = match option_env!("QUORUM_VERSION") {
    Some(v) => v,
    None => "dev",
};
