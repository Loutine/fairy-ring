use ricq::Client;
use std::sync::Arc;

use color_eyre::eyre::{self, ContextCompat, WrapErr};
use matrix_sdk_appservice::{
    matrix_sdk::{event_handler::Ctx, room::Room},
    ruma::{
        api::client,
        events::room::message::{MessageType, RoomMessageEventContent, SyncRoomMessageEvent},
        OwnedServerName, RoomOrAliasId,
    },
    AppService, AppServiceRegistration,
};

use crate::{
    config::{self, CONFIG},
    qq, MatrixMessage, QQMessage,
};

const USER_PREFIX: &str = "_qq_";

pub async fn new_appservice() -> eyre::Result<AppService> {
    let config = get_config()?;
    let homeserver_url = config.homeserver_url.parse()?;
    let server_name: OwnedServerName = config.homeserver_name.parse()?;
    let registration = AppServiceRegistration::try_from_yaml_file("./registration.yaml")
        .wrap_err("`registration.yaml` not found")?;

    let appservice = AppService::builder(homeserver_url, server_name.clone(), registration)
        .build()
        .await
        .wrap_err("Failed to build the AppService")?;

    Ok(appservice)
}

pub async fn run_appservice(
    appservice: AppService,
    qq_client: Arc<Client>,
) -> eyre::Result<()> {
    // register the main user
    reg_user(&appservice, None).await?;

    appservice
        .register_user_query(Box::new(|_, req| Box::pin(async move{
	    tracing::info!("Got request for User {}", req.user_id);
	    true
	})))
        .await;
    
    appservice
        .register_room_query(Box::new(|_, req| Box::pin(async move {
	    tracing::info!("Got request for Room {}", req.room_alias);
	    true
	})))
        .await;
    
    let client = appservice
        .user(None)
        .await
        .wrap_err("Failed to create a client from AppService")?;
    
    client.add_event_handler_context(qq_client);
    client.add_event_handler_context(appservice.clone());
    client.add_event_handler(handle_message);

    let (host, port) = appservice.registration().get_host_and_port()?;

    appservice
        .run(host, port)
        .await
        .wrap_err("Service failed")?;

    Ok(())
}

fn get_config() -> eyre::Result<&'static config::Matrix> {
    Ok(&CONFIG.get().wrap_err("Config not initialized")?.matrix)
}

async fn reg_user(appservice: &AppService, localpart: Option<&str>) -> eyre::Result<()> {
    let localpart =
        localpart.unwrap_or_else(|| appservice.registration().sender_localpart.as_str());
    match appservice.register_user(localpart, None).await {
        Ok(_) => (),
        // Ok if user has already been registered
        Err(matrix_sdk_appservice::Error::Matrix(e))
            if e.client_api_error_kind() == Some(&client::error::ErrorKind::UserInUse) => {}
        e => e?,
    }
    Ok(())
}

#[tracing::instrument(skip(ev, room, svc, qq_client))]
async fn handle_message(
    ev: SyncRoomMessageEvent,
    room: Room,
    Ctx(svc): Ctx<AppService>,
    Ctx(qq_client): Ctx<Arc<Client>>,
) -> eyre::Result<()> {
    if let SyncRoomMessageEvent::Original(ev) = ev {
        let Some(group_id) = room.room_id().localpart().strip_prefix("_qq_") else {
            return Ok(())
        };
        if !svc.user_id_is_in_namespace(&ev.sender) {
            if let MessageType::Text(t) = ev.content.msgtype {
                let msg = MatrixMessage {
                    group_id: group_id.parse().wrap_err_with(|| {
                        format!("Failed to parse group_id; input was {group_id}")
                    })?,
                    username: ev.sender.into(),
                    content: t.body,
                };

                qq::send_message(&qq_client, msg).await?;
            }
        }
    }

    Ok(())
}

// transit QQ message to Matrix
pub(crate) async fn send_message(appservice: &AppService, msg: QQMessage) -> eyre::Result<()> {
    let QQMessage {
        group_id,
        user_id,
        // username, // TODO: handle username
        content,
        ..
    } = msg;
    
    // TODO: Users in the group should be registered before-hand
    let user_name = virtual_user_name(user_id);
    
    let e: eyre::Result<_> = try {appservice.register_user(&user_name, None).await};
    if let Err(e) = e {
	tracing::error!("Can't register user: {e}");
    }
    let user = appservice.user(Some(&user_name)).await?;

    let homeserver: &str = &get_config()?.homeserver_name;

    let room_alias = RoomOrAliasId::parse(format!("#_qq_{group_id}:{homeserver}"))?;
    let room = user
        .join_room_by_id_or_alias(&room_alias, &[homeserver.parse()?])
        .await
        .wrap_err_with(|| format!("Virtual user {user_id} failed to join room {room_alias}"))?;

    // TODO: Send state event to set room nick

    let message = RoomMessageEventContent::text_plain(content);

    room.send(message, None).await?;

    Ok(())
}

pub fn virtual_user_name(id: i64) -> String {
    format!("{}{}", USER_PREFIX, id.to_string())
}
