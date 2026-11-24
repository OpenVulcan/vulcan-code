//! Implements the OpenAI public-client sign-in and token lifecycle for ChatGPT plan usage.
//! 为 ChatGPT 套餐用量实现 OpenAI 公共客户端登录和令牌生命周期。

use super::{
    Client, CredentialSecretMaterial, OAUTH_HTTP_REQUEST_TIMEOUT, OAUTH_TRANSACTION_TIMEOUT,
    OAuthAuthorizationChallenge, OAuthAuthorizationFlowKind, OAuthAuthorizationRuntime,
    OAuthTokenSet, OAuthTransactionPhase, OAuthTransactionSnapshot, TcpListener, URL_SAFE_NO_PAD,
    Url, Uuid, build_pkce_challenge, callback_html, decode_bounded_oauth_json_response,
    finish_transaction, oneshot, read_callback_headers, set_transaction_phase, timeout,
    unix_millis, write_callback_response,
};
use base64::Engine as _;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

/// Stable identifier for the official plan service entry.
/// 官方套餐服务入口的稳定标识。
pub const OPENAI_CHATGPT_PLAN_ENTRY_ID: &str = "chatgpt-plan";
/// Resource audience and API base for the issued subscription credential.
/// 套餐凭据的资源受众及 API 基地址。
pub const OPENAI_CHATGPT_PLAN_BASE_URL: &str = "https://api.openai.com/v1";
/// Issuer and fixed public endpoints published in its OIDC configuration.
/// OpenID 配置中发布的签发者及固定公共端点。
const ISSUER: &str = "https://auth.openai.com";
const AUTHORIZE_URL: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
const JWKS_URL: &str = "https://auth.openai.com/.well-known/jwks.json";
const REVOKE_URL: &str = "https://auth.openai.com/api/accounts/oauth/revoke";
/// Initial dynamic registration ID, callback path and required scope.
/// 初次动态注册 ID、回调路径及必要授权范围。
const INITIAL_CLIENT: &str = "dynamic_agent_client";
const CALLBACK_PATH: &str = "/auth/callback";
const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
const INVOKE_SCOPE: &str = "resource.invoke";

/// Uses only HTTPS and refuses redirects that could forward token form data.
/// 仅使用 HTTPS，并拒绝可能转发令牌表单的重定向。
fn oauth_client() -> Result<&'static Client, String> {
    static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| String::from("Could not initialize ChatGPT authorization transport."))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Returns whether the provider and entry identify the official ChatGPT plan.
/// 返回供应商及服务入口是否指向官方 ChatGPT 套餐。
pub fn is_openai_chatgpt_plan(provider_id: &str, entry_id: &str) -> bool {
    provider_id == "openai" && entry_id == OPENAI_CHATGPT_PLAN_ENTRY_ID
}

/// Holds a single callback's proofs and, for reauthorization, its expected account.
/// 保存单次回调的验证信息，并在重新授权时标记预期账号。
struct PlanSignIn {
    state: String,
    nonce: String,
    verifier: String,
    redirect_uri: String,
    registration: Option<CredentialSecretMaterial>,
}

/// Decodes the documented token reply without leaking its body into diagnostics.
/// 解码官方令牌响应，避免响应正文进入诊断信息。
#[derive(Deserialize)]
struct PlanTokenReply {
    access_token: String,
    refresh_token: String,
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    scope: Option<String>,
}

/// Carries verified identity claims used to bind the resulting credential.
/// 保存用于绑定凭据的已验证身份声明。
#[derive(Deserialize)]
struct PlanIdentity {
    iss: String,
    aud: String,
    sub: String,
    exp: u64,
    nonce: Option<String>,
    email: Option<String>,
}

/// Selects the signed JWT's documented algorithm and verification key.
/// 指定 JWT 声明的签名算法和验证密钥。
#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
    kid: String,
}

/// Describes the issuer's published RSA verification keys.
/// 描述签发者发布的 RSA 验签密钥。
#[derive(Deserialize)]
struct JsonWebKeySet {
    keys: Vec<JsonWebKey>,
}

/// Represents one verification key in the OpenAI JWKS response.
/// 表示 OpenAI JWKS 响应中的一把验签密钥。
#[derive(Deserialize)]
struct JsonWebKey {
    kid: String,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

impl OAuthAuthorizationRuntime {
    /// Starts a loopback sign-in using the local runtime ID and optional existing registration.
    /// 使用本机运行标识和可选的既有注册启动回环登录。
    pub async fn start_openai_chatgpt(
        &self,
        authorization_binding: String,
        host_id: Uuid,
        registration: Option<CredentialSecretMaterial>,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|_| String::from("Could not reserve the ChatGPT loopback callback."))?;
        let port = listener
            .local_addr()
            .map_err(|_| String::from("Could not read the ChatGPT loopback callback address."))?
            .port();
        let attempt = PlanSignIn {
            state: random_proof(),
            nonce: random_proof(),
            verifier: random_proof(),
            redirect_uri: format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
            registration,
        };
        let authorization_url = authorization_url(host_id, &attempt)?;
        let transaction_id = format!("auth_{}", Uuid::new_v4().simple());
        let snapshot = OAuthTransactionSnapshot {
            transaction_id: transaction_id.clone(),
            authorization_binding,
            phase: OAuthTransactionPhase::WaitingForCallback,
            expires_at_unix_ms: unix_millis()?
                .saturating_add(OAUTH_TRANSACTION_TIMEOUT.as_millis() as u64),
            message: None,
        };
        let transactions = Arc::clone(&self.transactions);
        let task_id = transaction_id.clone();
        let client = oauth_client()?;
        let (start, ready) = oneshot::channel();
        let task = tokio::spawn(async move {
            if ready.await.is_err() {
                return;
            }
            let result = async {
                let (code, client_id) = timeout(
                    OAUTH_TRANSACTION_TIMEOUT,
                    receive_callback(&listener, &attempt),
                )
                .await
                .map_err(|_| String::from("ChatGPT sign-in timed out."))??;
                set_transaction_phase(
                    &transactions,
                    &task_id,
                    OAuthTransactionPhase::FinalizingCredential,
                )
                .await;
                exchange_code(client, &attempt, &client_id, &code).await
            }
            .await;
            finish_transaction(transactions, &task_id, result).await;
        });
        self.register_transaction(&transaction_id, snapshot.clone(), &task)
            .await?;
        if start.send(()).is_err() {
            self.transactions.lock().await.remove(&transaction_id);
            return Err(String::from(
                "ChatGPT callback stopped before registration.",
            ));
        }
        Ok(OAuthAuthorizationChallenge {
            transaction: snapshot,
            authorization_url: authorization_url.into(),
            flow_kind: OAuthAuthorizationFlowKind::LoopbackCallback,
            user_code: None,
            poll_interval_ms: None,
        })
    }

    /// Revokes the selected registration's refresh token after local requests have been disabled.
    /// 本地请求停用后撤销所选注册的刷新令牌。
    pub async fn revoke_openai_chatgpt(
        &self,
        material: &CredentialSecretMaterial,
    ) -> Result<(), String> {
        let CredentialSecretMaterial::OpenAiChatGptOAuth {
            client_id, tokens, ..
        } = material
        else {
            return Err(String::from(
                "Expected a ChatGPT Plan credential for revocation.",
            ));
        };
        let response = oauth_client()?.post(REVOKE_URL)
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
            .form(&[
                ("token", tokens.refresh_token.as_str()),
                ("token_type_hint", "refresh_token"),
                ("client_id", client_id.as_str()),
            ])
            .send().await
            .map_err(|_| String::from("ChatGPT could not confirm revocation. Disconnect Vulcan Code in ChatGPT Settings."))?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(String::from(
                "ChatGPT could not confirm revocation. Disconnect Vulcan Code in ChatGPT Settings.",
            ));
        }
        Ok(())
    }
}

/// Generates independent PKCE, state and nonce values.
/// 生成相互独立的 PKCE、state 和 nonce 值。
fn random_proof() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Builds the initial or returning OpenAI authorization request.
/// 构造初次注册或已有账号的 OpenAI 授权请求。
fn authorization_url(host_id: Uuid, attempt: &PlanSignIn) -> Result<Url, String> {
    let mut url = Url::parse(AUTHORIZE_URL)
        .map_err(|_| String::from("Invalid ChatGPT authorization URL."))?;
    let mut query = url.query_pairs_mut();
    match attempt.registration.as_ref() {
        Some(CredentialSecretMaterial::OpenAiChatGptOAuth {
            tokens,
            client_id,
            id_token,
        }) => {
            query
                .append_pair("client_id", client_id)
                .append_pair("id_token_hint", id_token);
            if let Some(email) = tokens.email.as_deref() {
                query.append_pair("login_hint", email);
            }
        }
        None => {
            query
                .append_pair("client_id", INITIAL_CLIENT)
                .append_pair("agent_name_hint", "Vulcan Code");
        }
        Some(_) => {
            return Err(String::from(
                "The selected credential is not a ChatGPT Plan registration.",
            ));
        }
    }
    query
        .append_pair("ext_agent_host_id", &host_id.urn().to_string())
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &attempt.redirect_uri)
        .append_pair(
            "scope",
            "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
        )
        .append_pair("resource", OPENAI_CHATGPT_PLAN_BASE_URL)
        .append_pair("state", &attempt.state)
        .append_pair("nonce", &attempt.nonce)
        .append_pair("code_challenge", &build_pkce_challenge(&attempt.verifier))
        .append_pair("code_challenge_method", "S256");
    drop(query);
    Ok(url)
}

/// Waits for a callback with the exact state, ignoring unrelated loopback requests.
/// 等待状态精确匹配的回调，忽略无关的回环请求。
async fn receive_callback(
    listener: &TcpListener,
    attempt: &PlanSignIn,
) -> Result<(String, String), String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|_| String::from("ChatGPT callback listener failed."))?;
        let Ok(Ok(headers)) = timeout(
            OAUTH_HTTP_REQUEST_TIMEOUT,
            read_callback_headers(&mut stream),
        )
        .await
        else {
            continue;
        };
        let result = parse_callback(&headers, attempt);
        let _ = timeout(
            OAUTH_HTTP_REQUEST_TIMEOUT,
            write_callback_response(&mut stream, callback_html(matches!(result, Ok(Some(_))))),
        )
        .await;
        match result {
            Ok(Some(callback)) => return Ok(callback),
            Ok(None) => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Validates callback method, path, state and issued client before returning the code.
/// 在返回授权码前校验回调方法、路径、状态及签发的客户端。
fn parse_callback(
    headers: &[u8],
    attempt: &PlanSignIn,
) -> Result<Option<(String, String)>, String> {
    let Some(line) = std::str::from_utf8(headers)
        .ok()
        .and_then(|request| request.lines().next())
    else {
        return Ok(None);
    };
    let mut request_line = line.split_ascii_whitespace();
    let (Some("GET"), Some(target)) = (request_line.next(), request_line.next()) else {
        return Ok(None);
    };
    if !target.starts_with("/auth/callback?") {
        return Ok(None);
    }
    let parsed = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| String::from("ChatGPT callback URL is invalid."))?;
    let mut query = BTreeMap::new();
    for (key, value) in parsed.query_pairs() {
        if query.insert(key, value).is_some() {
            return Ok(None);
        }
    }
    if query.get("state").map(|state| state.as_ref()) != Some(attempt.state.as_str()) {
        return Ok(None);
    }
    if query.contains_key("error") {
        return Err(String::from(
            "ChatGPT authorization was declined or is unavailable for this account.",
        ));
    }
    let code = query
        .get("code")
        .filter(|code| !code.is_empty())
        .ok_or_else(|| String::from("ChatGPT callback did not supply an authorization code."))?;
    let returned_client = query.get("client_id").map(|id| id.as_ref());
    let client_id = match attempt.registration.as_ref() {
        Some(CredentialSecretMaterial::OpenAiChatGptOAuth { client_id, .. })
            if returned_client.is_none_or(|returned| returned == client_id) =>
        {
            client_id.as_str()
        }
        None => returned_client
            .filter(|id| !id.is_empty() && *id != INITIAL_CLIENT)
            .ok_or_else(|| String::from("ChatGPT callback did not supply an issued client ID."))?,
        _ => {
            return Err(String::from(
                "ChatGPT callback changed the selected registration.",
            ));
        }
    };
    Ok(Some((code.to_string(), client_id.to_string())))
}

/// Exchanges one validated code using the issued client ID and the original redirect URI.
/// 使用已签发的客户端标识和原回调地址交换一次已验证的授权码。
async fn exchange_code(
    client: &Client,
    attempt: &PlanSignIn,
    client_id: &str,
    code: &str,
) -> Result<CredentialSecretMaterial, String> {
    let reply = token_request(
        client,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", &attempt.verifier),
            ("redirect_uri", &attempt.redirect_uri),
            ("resource", OPENAI_CHATGPT_PLAN_BASE_URL),
        ],
    )
    .await?;
    let id_token = reply
        .id_token
        .clone()
        .ok_or_else(|| String::from("ChatGPT did not supply an ID token."))?;
    let identity = verify_identity(client, &id_token, client_id, Some(&attempt.nonce)).await?;
    if let Some(CredentialSecretMaterial::OpenAiChatGptOAuth { tokens, .. }) =
        attempt.registration.as_ref()
        && tokens.account_subject.as_deref() != Some(identity.sub.as_str())
    {
        return Err(String::from(
            "Sign in with the ChatGPT account already linked to this connection.",
        ));
    }
    Ok(CredentialSecretMaterial::OpenAiChatGptOAuth {
        tokens: token_set(reply, identity.sub, identity.email, None)?,
        client_id: client_id.to_owned(),
        id_token,
    })
}

/// Renews one account using its saved issued client ID and rotates its complete token set.
/// 使用账号保存的客户端标识续期，并轮换完整的令牌集合。
pub(super) async fn refresh(
    material: &CredentialSecretMaterial,
) -> Result<CredentialSecretMaterial, String> {
    let client = oauth_client()?;
    let CredentialSecretMaterial::OpenAiChatGptOAuth {
        tokens,
        client_id,
        id_token,
    } = material
    else {
        return Err(String::from(
            "Expected a ChatGPT Plan credential for refresh.",
        ));
    };
    let subject = tokens
        .account_subject
        .as_deref()
        .ok_or_else(|| String::from("ChatGPT registration has no verified subject."))?;
    let reply = token_request(
        client,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", &tokens.refresh_token),
            ("resource", OPENAI_CHATGPT_PLAN_BASE_URL),
        ],
    )
    .await?;
    let next_id = if let Some(next_id) = reply.id_token.as_deref() {
        let identity = verify_identity(client, next_id, client_id, None).await?;
        if identity.sub != subject {
            return Err(String::from(
                "ChatGPT refresh changed the selected registration.",
            ));
        }
        next_id.to_owned()
    } else {
        id_token.clone()
    };
    Ok(CredentialSecretMaterial::OpenAiChatGptOAuth {
        tokens: token_set(
            reply,
            subject.to_owned(),
            tokens.email.clone(),
            Some(&tokens.scopes),
        )?,
        client_id: client_id.clone(),
        id_token: next_id,
    })
}

/// Sends a bounded token POST without logging credentials or the upstream body.
/// 发送有界的令牌 POST，避免记录凭据及上游响应正文。
async fn token_request(client: &Client, form: &[(&str, &str)]) -> Result<PlanTokenReply, String> {
    let response = client
        .post(TOKEN_URL)
        .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
        .form(form)
        .send()
        .await
        .map_err(|_| String::from("Could not reach the ChatGPT authorization service."))?;
    if !response.status().is_success() {
        return Err(format!(
            "ChatGPT token exchange returned HTTP {}. Sign in again if the grant expired.",
            response.status().as_u16()
        ));
    }
    decode_bounded_oauth_json_response(response, "ChatGPT token response").await
}

/// Validates token type, permissions and lifetime before constructing a persisted token set.
/// 在构造持久令牌集合前校验类型、授权范围和有效期。
fn token_set(
    reply: PlanTokenReply,
    subject: String,
    email: Option<String>,
    old_scopes: Option<&BTreeSet<String>>,
) -> Result<OAuthTokenSet, String> {
    let scopes = match reply.scope {
        Some(scope) => scope.split_ascii_whitespace().map(str::to_owned).collect(),
        None => old_scopes
            .cloned()
            .ok_or_else(|| String::from("ChatGPT plan permissions were not granted."))?,
    };
    if !reply.token_type.eq_ignore_ascii_case("bearer")
        || !scopes.contains(PLAN_SCOPE)
        || !scopes.contains(INVOKE_SCOPE)
    {
        return Err(String::from("ChatGPT plan permissions were not granted."));
    }
    let issued_at_unix_ms = unix_millis()?;
    let expires_at_unix_ms = issued_at_unix_ms
        .checked_add(reply.expires_in.saturating_mul(1000))
        .ok_or_else(|| String::from("ChatGPT token lifetime is invalid."))?;
    let tokens = OAuthTokenSet {
        access_token: reply.access_token,
        refresh_token: reply.refresh_token,
        issued_at_unix_ms,
        expires_at_unix_ms,
        scopes,
        account_subject: Some(subject),
        email,
    };
    tokens.validate()?;
    Ok(tokens)
}

/// Verifies an ID token against the OpenAI JWKS and checks its expected login claims.
/// 使用 OpenAI JWKS 验签 ID 令牌并校验预期的登录声明。
async fn verify_identity(
    client: &Client,
    token: &str,
    client_id: &str,
    nonce: Option<&str>,
) -> Result<PlanIdentity, String> {
    let invalid = || String::from("ChatGPT ID token could not be verified.");
    let (signed_input, signature) = token.rsplit_once('.').ok_or_else(invalid)?;
    let (header, payload) = signed_input.split_once('.').ok_or_else(invalid)?;
    let header: JwtHeader =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
    if header.alg != "RS256" || header.kid.is_empty() {
        return Err(invalid());
    }
    let response = client
        .get(JWKS_URL)
        .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| invalid())?;
    if !response.status().is_success() {
        return Err(invalid());
    }
    let jwks: JsonWebKeySet =
        decode_bounded_oauth_json_response(response, "ChatGPT signing keys").await?;
    let key = jwks
        .keys
        .into_iter()
        .find(|key| key.kid == header.kid && key.kty == "RSA")
        .ok_or_else(invalid)?;
    let rsa = RsaPublicKeyComponents {
        n: URL_SAFE_NO_PAD
            .decode(key.n.ok_or_else(invalid)?)
            .map_err(|_| invalid())?,
        e: URL_SAFE_NO_PAD
            .decode(key.e.ok_or_else(invalid)?)
            .map_err(|_| invalid())?,
    };
    rsa.verify(
        &RSA_PKCS1_2048_8192_SHA256,
        signed_input.as_bytes(),
        &URL_SAFE_NO_PAD.decode(signature).map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())?;
    let identity: PlanIdentity =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
    if identity.iss != ISSUER
        || identity.aud != client_id
        || identity.exp <= unix_millis()? / 1000
        || identity.sub.trim().is_empty()
        || nonce.is_some_and(|expected| identity.nonce.as_deref() != Some(expected))
    {
        return Err(invalid());
    }
    Ok(identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Produces a deterministic pending login without opening a real browser.
    /// 构造确定性的等待中登录，无需打开真实浏览器。
    fn attempt() -> PlanSignIn {
        PlanSignIn {
            state: "fixture-state".into(),
            nonce: "fixture-nonce".into(),
            verifier: "fixture-verifier".into(),
            redirect_uri: format!("http://127.0.0.1:14555{CALLBACK_PATH}"),
            registration: None,
        }
    }

    /// Verifies first login declares the OpenAI resource, app identity, and PKCE.
    /// 验证首次登录声明 OpenAI 资源、应用身份和 PKCE。
    #[test]
    fn authorization_registers_the_local_app() {
        let host_id = Uuid::new_v4();
        let fixture = attempt();
        let url = authorization_url(host_id, &fixture).unwrap();
        let query = url.query_pairs().collect::<BTreeMap<_, _>>();
        assert_eq!(query["client_id"], INITIAL_CLIENT);
        assert_eq!(query["agent_name_hint"], "Vulcan Code");
        assert_eq!(query["ext_agent_host_id"], host_id.urn().to_string());
        assert_eq!(query["resource"], OPENAI_CHATGPT_PLAN_BASE_URL);
        assert_eq!(query["redirect_uri"], fixture.redirect_uri);
        assert_eq!(query["state"], fixture.state);
        assert_eq!(query["nonce"], fixture.nonce);
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(
            query["code_challenge"],
            build_pkce_challenge(&fixture.verifier)
        );
    }

    /// Verifies unrelated callbacks and incomplete registrations cannot yield an exchange code.
    /// 验证无关回调及不完整注册无法产生用于交换的授权码。
    #[test]
    fn callback_requires_exact_state_and_issued_client() {
        let fixture = attempt();
        let unrelated =
            b"GET /auth/callback?code=proof&state=other&client_id=oaiapp_test HTTP/1.1\r\n\r\n";
        assert_eq!(parse_callback(unrelated, &fixture).unwrap(), None);
        let duplicate = b"GET /auth/callback?code=proof&state=fixture-state&state=fixture-state&client_id=oaiapp_test HTTP/1.1\r\n\r\n";
        assert_eq!(parse_callback(duplicate, &fixture).unwrap(), None);
        let unissued = b"GET /auth/callback?code=proof&state=fixture-state&client_id=dynamic_agent_client HTTP/1.1\r\n\r\n";
        assert!(parse_callback(unissued, &fixture).is_err());
        let issued = b"GET /auth/callback?code=proof&state=fixture-state&client_id=oaiapp_test HTTP/1.1\r\n\r\n";
        assert_eq!(
            parse_callback(issued, &fixture).unwrap(),
            Some(("proof".into(), "oaiapp_test".into()))
        );
    }

    /// Rejects identity-only authorization even when the token itself is otherwise well formed.
    /// 即使令牌结构正确也拒绝仅含账号身份的授权。
    #[test]
    fn token_without_plan_permission_is_rejected() {
        let reply = PlanTokenReply {
            access_token: "fixture-access".into(),
            refresh_token: "fixture-refresh".into(),
            id_token: None,
            token_type: "Bearer".into(),
            expires_in: 3600,
            scope: Some("openid profile offline_access".into()),
        };
        assert!(token_set(reply, "fixture-subject".into(), None, None).is_err());
    }
}
