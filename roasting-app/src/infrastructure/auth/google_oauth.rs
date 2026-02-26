use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use serde::Deserialize;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub sub: String, // Google's unique user ID
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
}

// Type alias for the configured OAuth client
type ConfiguredClient = oauth2::Client<
    oauth2::basic::BasicErrorResponse,
    oauth2::basic::BasicTokenResponse,
    oauth2::basic::BasicTokenIntrospectionResponse,
    oauth2::StandardRevocableToken,
    oauth2::basic::BasicRevocationErrorResponse,
    oauth2::EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
>;

#[derive(Clone)]
pub struct GoogleOAuth {
    client: ConfiguredClient,
    redirect_uri: RedirectUrl,
    http_client: reqwest::Client,
}

impl GoogleOAuth {
    pub fn new(client_id: &str, client_secret: &str, redirect_uri: &str) -> Result<Self, String> {
        let auth_url = AuthUrl::new(GOOGLE_AUTH_URL.to_string()).map_err(|e| e.to_string())?;
        let token_url = TokenUrl::new(GOOGLE_TOKEN_URL.to_string()).map_err(|e| e.to_string())?;
        let redirect = RedirectUrl::new(redirect_uri.to_string()).map_err(|e| e.to_string())?;

        let client = BasicClient::new(ClientId::new(client_id.to_string()))
            .set_client_secret(ClientSecret::new(client_secret.to_string()))
            .set_auth_uri(auth_url)
            .set_token_uri(token_url);

        let http_client = reqwest::Client::new();

        Ok(Self {
            client,
            redirect_uri: redirect,
            http_client,
        })
    }

    /// Generate the authorization URL and PKCE verifier
    pub fn get_auth_url(&self) -> (String, CsrfToken, PkceCodeVerifier) {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, csrf_token) = self
            .client
            .authorize_url(CsrfToken::new_random)
            .set_redirect_uri(std::borrow::Cow::Borrowed(&self.redirect_uri))
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        (auth_url.to_string(), csrf_token, pkce_verifier)
    }

    /// Exchange the authorization code for tokens and fetch user info
    pub async fn exchange_code(
        &self,
        code: &str,
        pkce_verifier: PkceCodeVerifier,
    ) -> Result<GoogleUserInfo, String> {
        // Build the HTTP client for oauth2
        let http_client = oauth2::reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        // Exchange code for tokens
        let token_result = self
            .client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .set_redirect_uri(std::borrow::Cow::Borrowed(&self.redirect_uri))
            .set_pkce_verifier(pkce_verifier)
            .request_async(&http_client)
            .await
            .map_err(|e| format!("Token exchange failed: {:?}", e))?;

        let access_token = token_result.access_token().secret();

        // Fetch user info
        let user_info = self
            .http_client
            .get(GOOGLE_USERINFO_URL)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch user info: {}", e))?
            .json::<GoogleUserInfo>()
            .await
            .map_err(|e| format!("Failed to parse user info: {}", e))?;

        Ok(user_info)
    }
}
