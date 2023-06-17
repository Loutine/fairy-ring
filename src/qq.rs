use std::{fs::File, future::Future, path::Path, pin::Pin, sync::Arc, time::Duration};

use bytes::Bytes;
use color_eyre::eyre;
use matrix_sdk_appservice::AppService;
// TODO: remove proc_qq
use proc_qq::re_exports::{
    ricq::{
        client::{event::GroupMessageEvent, Connector, DefaultConnector},
        ext::common::after_login,
        handler::{Handler, QEvent},
        Client, Device, LoginResponse, Protocol, QRCodeConfirmed, QRCodeImageFetch, QRCodeState,
    },
    ricq_core::{
        msg::{elem::Text, MessageChain},
        structs::GroupMessage,
    },
    serde_json,
};

use crate::{matrix, MatrixMessage, QQMessage};

pub async fn new_client(appservice: AppService) -> eyre::Result<Arc<Client>> {
    let device = init_device()?;

    Ok(Arc::new(Client::new(
        device,
        Protocol::AndroidWatch.into(),
        MatrixForwarder::new(appservice),
    )))
}

#[tracing::instrument(skip(client))]
pub async fn run_client(client: Arc<Client>) -> eyre::Result<()> {
    let stream = DefaultConnector.connect(&client).await?;
    let handle = {
        let client = client.clone();
        tokio::spawn(async move { client.start(stream).await })
    };

    tokio::task::yield_now().await;
    qr_login(&client).await?;
    after_login(&client).await;
    handle.await?;

    Ok(())
}

fn init_device() -> eyre::Result<Device> {
    let device_path = Path::new("device.json");
    let device = if device_path.exists() {
        serde_json::from_reader(File::open(device_path)?)?
    } else {
        let device = Device::random();
        serde_json::to_writer(
            File::options().write(true).create(true).open(device_path)?,
            &device,
        )?;
        device
    };
    Ok(device)
}

async fn qr_login(client: &Client) -> Result<(), color_eyre::Report> {
    let mut resp = client.fetch_qrcode().await?;
    let mut qr_sig = Bytes::new();
    loop {
        match &resp {
            QRCodeState::ImageFetch(QRCodeImageFetch { image_data, sig }) => {
                qr_sig = sig.clone();
                tokio::fs::write("qrcode.png", image_data).await?;
                tracing::info!("Wrote QR code to `qrcode.png`");
            }
            QRCodeState::Timeout => {
                tracing::warn!("Login timeout, requesting new QR");
                resp = client.fetch_qrcode().await?;
                // Do not query QR result
                continue;
            }
            QRCodeState::Confirmed(QRCodeConfirmed {
                tmp_pwd,
                tmp_no_pic_sig,
                tgt_qr,
                ..
            }) => {
                let mut login_resp = client.qrcode_login(tmp_pwd, tmp_no_pic_sig, tgt_qr).await?;
                if let LoginResponse::DeviceLockLogin { .. } = login_resp {
                    login_resp = client.device_lock_login().await?;
                }
                tracing::info!("{login_resp:?}");
                break Ok(());
            }
            QRCodeState::Canceled => panic!("Login is canceled"),
            _ => (),
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
        resp = client.query_qrcode_result(&qr_sig).await?;
    }
}

struct MatrixForwarder {
    appservice: AppService,
}

impl MatrixForwarder {
    fn new(appservice: AppService) -> Self {
        MatrixForwarder { appservice }
    }
}

impl Handler for MatrixForwarder {
    fn handle<'a: 'b, 'b>(
        &'a self,
        event: QEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'b>> {
        Box::pin(async move {
            if let QEvent::GroupMessage(ev) = event {
                // TODO: filter by group id
                let e: eyre::Result<()> = try {
                    let msg = extract_message(&ev).await?;
                    matrix::send_message(&self.appservice, msg).await?;
                };

                if let Err(e) = e {
                    tracing::error!("{e:?}");
                }
            }
        })
    }
}

async fn extract_message(ev: &GroupMessageEvent) -> eyre::Result<QQMessage> {
    let GroupMessageEvent {
        client,
        inner:
            GroupMessage {
                group_code,
                from_uin,
                elements,
                ..
            },
    } = ev;
    let group_id = *group_code;
    let user_id = *from_uin;
    // TODO: image
    let content = elements.to_string();
    let username = client
        .get_group_member_info(group_id, user_id)
        .await?
        .nickname;

    // TODO: avatar

    Ok(QQMessage {
        group_id,
        user_id,
        username,
        content,
    })
}

pub(crate) async fn send_message(client: &Client, msg: MatrixMessage) -> eyre::Result<()> {
    let message_chain = MessageChain::new(Text::new(format!("{}: {}", msg.username, msg.content)));

    client
        .send_group_message(msg.group_id, message_chain)
        .await?;

    Ok(())
}
