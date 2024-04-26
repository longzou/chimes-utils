// #[macro_use]
// extern crate wechat;
#[macro_use]
extern crate lazy_static;
// use awc::Client;

mod utils;
pub use utils::*;

mod errors;
pub use errors::*;

pub type ChimesResult<T> = Result<T, ChimesError>;
pub type Result<T, E = ChimesError> = core::result::Result<T, E>;

mod wechat;
pub use wechat::*;
