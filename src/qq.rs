use color_eyre::eyre;
use matrix_sdk_appservice::{ruma::events::room::message::RoomMessageEventContent, AppService};
use ricq::{
    client::{event::GroupMessageEvent, Connector, DefaultConnector},
    ext::common::after_login,
    handler::PartlyHandler,
    msg::elem::RQElem,
    structs::GroupMessage,
    Client, Protocol,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

mod login;

use crate::{config, matrix, MatrixMessage, QQMessage, QQMessageType};

//#[tracing::instrument(skip(client))]
pub async fn run_client(client: Arc<Client>) -> eyre::Result<()> {
    let stream = DefaultConnector.connect(&client).await?;
    let handle = {
        let client = client.clone();
        tokio::spawn(async move { client.start(stream).await })
    };

    tokio::task::yield_now().await;
    login::qr_login(&client).await?;
    after_login(&client).await;
    check_if_group_joined(&client).await;
    handle.await?;

    Ok(())
}

pub async fn new_client(appservice: AppService) -> eyre::Result<Arc<Client>> {
    let device = login::init_device()?;

    Ok(Arc::new(Client::new(
        device,
        Protocol::AndroidWatch.into(),
        MatrixForwarder::new(appservice),
    )))
}

// check if bot have already joined groups write in config file
async fn check_if_group_joined(client: &Arc<Client>) {
    let group_list: Vec<i64> = client
        .get_group_list()
        .await
        .expect("Can't get group list: ")
        .iter()
        .map(|g| g.uin)
        .collect();

    let group_in_config = config::CONFIG
        .get()
        .expect("Can't get config")
        .qq
        .groups
        .clone();

    let group_not_join: Vec<i64> = group_in_config
        .into_iter()
        .filter(|g| !group_list.contains(g))
        .collect();
    // get the group haven't joined and print
    client
        .get_group_infos(group_not_join)
        .await
        .expect("Can't get group info: ")
        .iter()
        .for_each(|g| tracing::info!("未加入群聊 group name:{}, group id: {}", g.name, g.uin));
}

// message transit

struct MatrixForwarder {
    appservice: AppService,
}

impl MatrixForwarder {
    fn new(appservice: AppService) -> Self {
        MatrixForwarder { appservice }
    }
}

impl PartlyHandler for MatrixForwarder {
    fn handle_group_message<'life0, 'async_trait>(
        &'life0 self,
        _event: GroupMessageEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let GroupMessageEvent {
                client,
                inner:
                    GroupMessage {
                        group_code, // group id
                        from_uin,   // user id
                        elements,   // message
                        ..
                    },
            } = _event;
	    let mut text_msg: String = String::new();
	    let mut msg_send: Vec<QQMessageType> = Vec::new();
            // filter group id
            if !config::CONFIG.get().unwrap().qq.groups.contains(&group_code) {
                return ();
            }
            for elem in elements.0 {
                match RQElem::from(elem) {
                    RQElem::Text(text) => text_msg += &text.content,
		    RQElem::Face(face) => text_msg += &face.to_string(),
		    RQElem::MarketFace(face) => text_msg += &format!("[表情:{}]", face.name),
		    RQElem::GroupImage(img) => {
			if !text_msg.is_empty() {
			    msg_send.push(QQMessageType::Text(text_msg.clone()));
			    text_msg.clear();
			}
			msg_send.push(QQMessageType::Img(img.url()))
		    },
		    // RQElem::VideoFile(vid) => vid,
		    // RQElem::At(_) => todo!(),
                    // RQElem::Dice(_) => todo!(),
                    // RQElem::FingerGuessing(_) => todo!(),
                    // RQElem::LightApp(_) => todo!(),
                    // RQElem::RichMsg(_) => todo!(),
                    // RQElem::FriendImage(_) => todo!(),
                    // RQElem::FlashImage(_) => todo!(),
                    // RQElem::Other(_) => todo!(),
		    _ => continue,
                }
            }
	    if !text_msg.is_empty() {
		msg_send.push(QQMessageType::Text(text_msg))
	    }
	    for qmsg in msg_send.into_iter().map(|msg| create_qq_message(group_code, from_uin,msg)) {
		let e: eyre::Result<()> = matrix::send_message(&self.appservice, qmsg).await;
		if let Err(e) = e {
		    tracing::error!("{e:?}");
		}
	    }
        })
    }
}

fn create_qq_message(group_id: i64, user_id: i64, msg: QQMessageType) -> QQMessage {
    QQMessage {
        group_id,
        user_id,
        content: msg,
    }
}

// pub(crate) async fn send_message(client: &Client, msg: MatrixMessage) -> eyre::Result<()> {
//     let message_chain = MessageChain::new(Text::new(format!("{}: {}", msg.username, msg.content)));

//     client
//         .send_group_message(msg.group_id, message_chain)
//         .await?;

//     Ok(())
// }
