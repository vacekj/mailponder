// src/main.rs

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use chrono::{Duration, Utc};
use imap;
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl,
    AuthorizationCode, TokenResponse,
};
use url::Url;
use native_tls;

#[derive(Serialize, Deserialize)]
struct Config {
    client_id: String,
    client_secret: String,
    auth_url: String,
    token_url: String,
    redirect_url: String,
    imap_server: String,
    imap_port: u16,
    days_to_fetch: u32,
    output_file: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read config file
    let config: Config = serde_json::from_reader(File::open("config.json")?)?;

    // Set up the OAuth2 client
    let oauth_client = BasicClient::new(
        ClientId::new(config.client_id),
        Some(ClientSecret::new(config.client_secret)),
        AuthUrl::new(config.auth_url)?,
        Some(TokenUrl::new(config.token_url)?),
    )
        .set_redirect_uri(RedirectUrl::new(config.redirect_url)?);

    // Generate the authorization URL
    let (auth_url, _csrf_token) = oauth_client.authorize_url(|| "csrf_token".to_string()).url();

    println!("Open this URL in your browser:\n{}\n", auth_url);
    println!("After authorization, enter the code from the redirect URL:");

    let mut auth_code = String::new();
    std::io::stdin().read_line(&mut auth_code)?;
    let auth_code = AuthorizationCode::new(auth_code.trim().to_string());

    // Exchange the authorization code for an access token
    let token_result = oauth_client
        .exchange_code(auth_code)
        .request(oauth2::reqwest::http_client)?;

    let access_token = token_result.access_token().secret();

    // Connect to the server using TLS
    let tls = native_tls::TlsConnector::builder().build()?;
    let client = imap::connect(
        (config.imap_server.as_str(), config.imap_port),
        &config.imap_server,
        &tls,
    )?;

    // Authenticate using OAUTH2
    let mut imap_session = client.authenticate("XOAUTH2", |challenge| {
        format!("user={}^Aauth=Bearer {}^A^A", "", access_token)
    }).map_err(|e| e.0)?;

    // Select the INBOX
    imap_session.select("INBOX")?;

    // Calculate the date range
    let since_date = Utc::now() - Duration::days(config.days_to_fetch as i64);
    let date_query = format!("SINCE {}", since_date.format("%d-%b-%Y"));

    // Fetch emails
    let messages = imap_session.search(&date_query)?;

    // Open output file
    let mut output_file = File::create(&config.output_file)?;

    // Fetch and write each email
    for sequence_number in messages.iter() {
        let message = imap_session.fetch(sequence_number.to_string(), "RFC822")?;
        let body = message[0].body().expect("Message did not have a body!");
        output_file.write_all(body)?;
        output_file.write_all(b"\n\n")?;
    }

    // Close the IMAP session
    imap_session.logout()?;

    println!("Emails have been downloaded and saved to {}", config.output_file);

    Ok(())
}