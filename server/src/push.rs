use std::fs;
use web_push::*;

pub async fn send_push(
    subscription_json: &str,
    title: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let key_path =
        std::env::var("VAPID_PRIVATE_KEY_PATH").unwrap_or_else(|_| "vapid_private.pem".to_string());
    let subscription_info: SubscriptionInfo = serde_json::from_str(subscription_json)?;
    let sig_builder =
        VapidSignatureBuilder::from_pem(fs::File::open(key_path)?, &subscription_info)?.build()?;

    let mut builder = WebPushMessageBuilder::new(&subscription_info);
    let payload = serde_json::json!({ "title": title, "body": body }).to_string();
    builder.set_payload(ContentEncoding::Aes128Gcm, payload.as_bytes());
    builder.set_vapid_signature(sig_builder);

    let client = IsahcWebPushClient::new()?;
    client.send(builder.build()?).await?;
    Ok(())
}
