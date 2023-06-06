use std::env;

use color_eyre::eyre::{self, WrapErr};
use matrix_sdk_appservice::{
    matrix_sdk::{event_handler::Ctx, room::Room},
    ruma::{
        api::client,
        events::room::message::{RoomMessageEventContent, SyncRoomMessageEvent},
        OwnedServerName,
    },
    AppService, AppServiceRegistration,
};
use tracing::{info, debug};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    env::set_var(
        "RUST_LOG",
        "matrix_sdk=debug,matrix_sdk_appservice=debug,fairy_ring=debug",
    );
    tracing_subscriber::fmt::init();

    run_appservice().await?;

    Ok(())
}

async fn run_appservice() -> Result<(), color_eyre::Report> {
    let homeserver_url = "http://localhost:6167".parse()?;
    let server_name: OwnedServerName = "matrix.spore.ink".parse()?;
    let registration = AppServiceRegistration::try_from_yaml_file("./registration.yaml")
        .wrap_err("`registration.yaml` not found")?;
    let appservice = AppService::builder(homeserver_url, server_name.clone(), registration)
        .build()
        .await
        .wrap_err("Failed to build an AppService")?;
    reg_user(&appservice, None).await?;
    appservice
        .register_user_query(Box::new(|_, _| Box::pin(async { true })))
        .await;
    let client = appservice
        .user(None)
        .await
        .wrap_err("Failed to create a client from AppService")?;
    client.add_event_handler_context(appservice.clone());
    client.add_event_handler(handle_message);
    let (host, port) = appservice.registration().get_host_and_port()?;
    appservice
        .run(host, port)
        .await
        .wrap_err("Service failed when running")?;

    Ok(())
}

async fn reg_user(svc: &AppService, localpart: Option<&str>) -> eyre::Result<()> {
    let localpart = localpart.unwrap_or_else(|| svc.registration().sender_localpart.as_str());
    match svc.register_user(localpart, None).await {
        Ok(_) => (),
        Err(matrix_sdk_appservice::Error::Matrix(e))
            if e.client_api_error_kind() == Some(&client::error::ErrorKind::UserInUse) =>
        {
            ()
        }
        e => e?,
    }
    Ok(())
}

#[tracing::instrument(skip(ev, room, svc))]
async fn handle_message(
    ev: SyncRoomMessageEvent,
    room: Room,
    Ctx(svc): Ctx<AppService>,
) -> eyre::Result<()> {
    if let SyncRoomMessageEvent::Original(ev) = ev {
        if !svc.user_id_is_in_namespace(ev.sender) {
            let user = svc.user(None).await?;

            // even if already joined since no reliable way to test if user is in the room
            let joined_room = {
                debug!("Try to join room {}", room.room_id());
                user.join_room_by_id(room.room_id())
                    .await
                    .wrap_err_with(|| format!("Failed to join room {}", room.room_id()))?
            };

            let msg = RoomMessageEventContent::text_plain("received");

            debug!("Sending `received` message to {}", room.room_id());

            joined_room
                .send(msg.clone(), None)
                .await
                .wrap_err_with(|| {
                    format!(
                        "Failed to send message ```{}``` to room {}",
                        msg.body(),
                        room.room_id()
                    )
                })?;
        }
    }

    Ok(())
}
