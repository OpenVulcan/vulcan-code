# TokenRhythm（基元律动）账户登录已验证事实

> 文档性质：供应商事实记录，不是修改实施计划。
>
> 本文是 `manifests/tokenrhythm.json` 中 `account-auth` 凭据配置、`token_rhythm` 注册 OAuth 与
> `token_rhythm_wallet` 余额查询的事实依据。证据分层：
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
3. 本仓库决策：`crates/vulcan-provider-adapters/src/tokenrhythm.rs` 的 `CLIENT_ID` 与 `INTEGRATION_APP`
   是唯一定义，二者一起修改。当前值 `vulcan-code` 是向 TokenRhythm 申请的名称，**注册完成前账户登录不可用**；
   API Key 凭据不受影响。不得以其他产品的注册对外发布。

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

## 五、未实现

- 移除凭据时调用 `DELETE /api/v1/integrations/{应用名}/session` 吊销远端登录会话。
