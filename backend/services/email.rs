use std::env;

pub async fn send_magic_link_email(to_email: &str, verify_url: &str) -> Result<(), String> {
    let api_key = env::var("RESEND_API_KEY").map_err(|_| "RESEND_API_KEY not set".to_string())?;
    // While your domain isn't verified in Resend yet, this "from" address
    // must stay as onboarding@resend.dev and can only deliver to the email
    // you signed up to Resend with. Once you verify your domain (the
    // start@safely.sh subdomain you set up), switch this to your own address.
    let from_address =
        env::var("RESEND_FROM_EMAIL").unwrap_or_else(|_| "onboarding@resend.dev".to_string());
    let client = reqwest::Client::new();

    // Built with classic HTML-email techniques throughout - table-based
    // layout, inline styles only, web-safe bold fonts, thick solid
    // borders instead of a real box-shadow - since Outlook's rendering
    // engine (and several other clients) don't support CSS variables,
    // flexbox/grid, custom web fonts, or drop-shadows at all. This is
    // the same visual language as the site (cream background, thick
    // black borders, bold black type, gold + mint accents), rebuilt
    // with only what actually survives across real inboxes.
    let html_body = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Sign in to Safely</title>
</head>
<body style="margin:0;padding:0;background-color:#fbf7ed;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color:#fbf7ed;padding:32px 16px;">
<tr>
<td align="center">

<table role="presentation" width="480" cellpadding="0" cellspacing="0" style="max-width:480px;width:100%;background-color:#ffffff;border:3px solid #161616;border-radius:20px;">

<tr>
<td style="padding:32px 32px 8px 32px;">
<table role="presentation" cellpadding="0" cellspacing="0">
<tr>
<td style="background-color:#161616;border-radius:10px;width:34px;height:34px;text-align:center;vertical-align:middle;">
<span style="color:#35d0a6;font-size:18px;font-weight:900;font-family:'Arial Black', Arial, sans-serif;line-height:34px;">S</span>
</td>
<td style="padding-left:10px;font-size:20px;font-weight:900;color:#161616;font-family:'Arial Black', Arial, sans-serif;">Safely</td>
</tr>
</table>
</td>
</tr>

<tr>
<td style="padding:16px 32px 0 32px;">
<h1 style="margin:0;font-size:26px;line-height:1.2;font-weight:900;color:#161616;font-family:'Arial Black', Arial, sans-serif;">Sign in to Safely</h1>
</td>
</tr>

<tr>
<td style="padding:12px 32px 0 32px;">
<p style="margin:0;font-size:15px;line-height:1.6;color:#5d6066;font-family:Arial, Helvetica, sans-serif;">Click the button below to sign in. This link expires in <strong style="color:#161616;">15 minutes</strong> and can only be used once.</p>
</td>
</tr>

<tr>
<td style="padding:24px 32px 8px 32px;">
<table role="presentation" cellpadding="0" cellspacing="0">
<tr>
<td style="background-color:#ffb937;border:2px solid #161616;border-radius:12px;">
<a href="{verify_url}" style="display:inline-block;padding:14px 28px;font-size:15px;font-weight:800;color:#161616;text-decoration:none;font-family:Arial, Helvetica, sans-serif;">Sign in to Safely &rarr;</a>
</td>
</tr>
</table>
</td>
</tr>

<tr>
<td style="padding:20px 32px 0 32px;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color:#fef6e6;border:2px solid #f2b84c;border-radius:10px;">
<tr>
<td style="padding:12px 14px;font-size:12.5px;font-weight:700;color:#9a6300;font-family:Arial, Helvetica, sans-serif;">Expires in 15 minutes &middot; single use only</td>
</tr>
</table>
</td>
</tr>

<tr>
<td style="padding:24px 32px 32px 32px;">
<p style="margin:0;font-size:12.5px;line-height:1.6;color:#9a9a9a;font-family:Arial, Helvetica, sans-serif;">If you didn't request this email, you can safely ignore it &mdash; no changes will be made to your account.</p>
</td>
</tr>

</table>

<table role="presentation" width="480" cellpadding="0" cellspacing="0" style="max-width:480px;width:100%;">
<tr>
<td align="center" style="padding:20px 0 0 0;font-size:12px;color:#9a9a9a;font-family:Arial, Helvetica, sans-serif;">Safely &middot; Fraud detection for online marketplaces</td>
</tr>
</table>

</td>
</tr>
</table>
</body>
</html>"#
    );

    let body = serde_json::json!({
        "from": format!("Safely <{}>", from_address),
        "to": [to_email],
        "subject": "Sign in to Safely",
        "html": html_body,
    });

    let response = client
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Resend error ({}): {}", status, text));
    }

    Ok(())
}
