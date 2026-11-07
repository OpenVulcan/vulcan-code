# TokenRhythm（基元律动）账户登录已验证事实

> 文档性质：供应商事实记录，不是修改实施计划。
>
> **当前状态：已实现、未启用。** 账户登录（`token_rhythm` 注册 OAuth、`tokenrhythm_auth` 凭据材料）与
> 钱包余额（`token_rhythm_wallet`）的代码保留，但 `manifests/tokenrhythm.json` 不声明 `account-auth`
> 凭据配置，因此界面只提供 API Key 凭据与模型同步。待基元律动登记 Vulcan Code 的客户端后，把
> `crates/vulcan-provider-adapters/src/tokenrhythm.rs` 的客户端与集成应用名改为登记值，再在清单中加回该配置。
>
> 本文记录重新接入所需的事实依据。证据分层：
>
> - **官方客户端源码**：TokenRhythm 官方工具 OpenSquilla 的 `desktop/electron/src/tokenrhythm-account.ts`。
> - **真实调用**：2026-11-07 用 OpenSquilla 的客户端注册在 Debug 应用完成一次真实登录后的结果。
> - **本仓库决策**：由上述证据推出的产品边界。
>
> 未列入本文的行为一律视为未知，不得据接口形态推测。

## 一、客户端注册（当前阻塞项）

1. 授权端只接受已注册的 `client_id`；未注册时 `GET /api/oauth/authorize` 直接返回
   `400 {"error":"invalid_client"}`（2026-11-07 用 `vulcan-code` 实测）。
2. 每个注册绑定一个集成应用名，账户资源路径为 `/api/v1/integrations/{应用名}/…`；OpenSquilla 的注册是
   `opensquilla-desktop` ↔ `opensquilla`，托管密钥响应的 `app_id` 等于该应用名。
3. 本仓库决策：客户端标识与集成应用名只在一处定义且一起修改；申请的名称为 `vulcan-code`，注册完成前
   不接入账户登录。不得以其他产品的注册对外发布。

## 二、登录协议（官方客户端源码 + 真实调用）

1. 授权码 + PKCE（`S256`），回调为回环地址 `http://127.0.0.1:{随机端口}/oauth/callback`。
2. 授权地址 `https://tokenrhythm.studio/api/oauth/authorize`，参数：`response_type=code`、`client_id`、
   `redirect_uri`、`state`、`code_challenge`、`code_challenge_method`、`scope`、`installation_id`。
3. 令牌端 `POST /api/oauth/token`，表单体；`grant_type` 为 `authorization_code`（`code`、`redirect_uri`、
   `code_verifier`）或 `refresh_token`。
4. 权限范围：`profile.read wallet.read managed_key.read managed_key.obtain session.revoke offline_access`，
   实测全部授予。
5. 令牌响应：`token_type=Bearer`、`expires_in`、`access_token`、`refresh_token`、`scope`、`account_id`、
   `session_id`。实测访问令牌有效期 15 分钟；协议上限按官方客户端为 86400 秒。
6. 刷新令牌每次轮换：实测应用在到期前 5 分钟自动刷新，凭据修订号递增、刷新令牌更换，
   账户与会话标识不变。

## 三、托管密钥与推理（真实调用）

1. `POST /api/v1/integrations/{应用名}/managed-key/credential`（访问令牌鉴权）返回 `{code:0,data:{…}}`，
   含 `secret`、`key_id`、`generation`、`account_id`、`app_id`、`status=active`。
2. 托管密钥与普通 API Key 用法相同：`GET /v1/models` 返回 26 个模型，`POST /v1/chat/completions` 成功。
3. 本仓库决策：推理只用托管密钥，访问令牌只用于账户资源；刷新访问令牌时保留托管密钥。

## 四、钱包余额（真实调用）

1. `GET /api/v1/integrations/{应用名}/wallet` 返回 `account_id`、`currency`（实测 `CNY`）、`as_of` 与
   `balances{available, recharge, bonus, debt, frozen}`，金额为十进制字符串，`available = recharge + bonus`。
2. 本仓库决策：余额展示 `available`；`debt` 非零时附加提示；不生成额度窗口。

## 五、启用步骤

模型规则的作用域不限定凭据配置，声明即覆盖全部规则；图片工具目标仍只限 API Key，托管密钥调用图片接口未验证。
登记完成后：

1. 把 `tokenrhythm.rs` 的 `CLIENT_ID` 与 `INTEGRATION_APP` 改为登记值。
2. 在 `manifests/tokenrhythm.json` 的 `public-api` 入口 `credential_profiles` 首位加回下列配置，再执行
   `node scripts/provider-manifests-refresh.mjs`：

```json
{
  "credential_profile_id": "account-auth",
  "display_name": "TokenRhythm Account",
  "credential_mode": "auth",
  "authorization": {"kind": "registered_oauth", "provider": "token_rhythm"},
  "billing_kind": "provider_managed",
  "account_usage_query": "token_rhythm_wallet",
  "default_usage_scopes": ["interactive_agent", "internal_background", "external_service"]
}
```

## 六、未实现

- 移除凭据时调用 `DELETE /api/v1/integrations/{应用名}/session` 吊销远端登录会话。
