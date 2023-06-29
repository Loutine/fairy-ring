use std::fs::File;
use std::path::Path;
use std::time::Duration;

use bytes::Bytes;
use color_eyre::eyre;

use ricq::{
    Client,
    Device,
    LoginResponse,
    QRCodeConfirmed,
    QRCodeImageFetch,
    QRCodeState,
};

pub fn init_device() -> eyre::Result<Device> {
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

pub(crate) async fn qr_login(client: &Client) -> Result<(), color_eyre::Report> {
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
