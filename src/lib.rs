#![feature(try_blocks, async_fn_in_trait)]

pub mod config;
pub mod matrix;
pub mod qq;

enum QQMessageType {
    Text(String),
    Img(String),
}
// todo! refactor the media enum to contain the Mime info
struct QQMessage {
    group_id: i64,
    user_id: i64,
    display_name: String,
    // TODO: user avatar
    content: QQMessageType
}

enum MatrixMessageType {
    Text(String),
    Img(Vec<u8>),
}


// todo! as QQMessageType

/// Matrix message to transport to qq
struct MatrixMessage {
    group_id: i64,
    username: String,
    content: MatrixMessageType,
}
