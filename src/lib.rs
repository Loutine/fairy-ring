#![feature(try_blocks, async_fn_in_trait)]

pub mod config;
pub mod matrix;
pub mod qq;

struct QQMessage {
    group_id: i64,
    user_id: i64,
    username: String,
    // TODO: user avatar
    content: String,
}

/// Matrix message to transport to qq
struct MatrixMessage {
    group_id: i64,
    username: String,
    content: String,
}
