//! MCP settings and OAuth message handling for the workbench update loop.
//! 工作台更新循环中的 MCP 设置与 OAuth 消息处理。

use super::*;

impl VulcanWorkbench {
    /// Handles MCP settings and OAuth messages outside the main update dispatcher.
    /// 在主更新分发器之外处理 MCP 设置与 OAuth 消息。
    pub(super) fn update_mcp_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::McpServerSelected(choice) => {
                self.selected_mcp_server_id = Some(choice.id.clone());
                self.reload_selected_mcp_server_form();
                self.mcp_server_editor_open = true;
                self.mcp_oauth_callback_input.clear();
                self.mcp_oauth_authorization_url.clear();
                self.mcp_oauth_pending_state = None;
                self.mcp_oauth_flow_in_flight = false;
                self.mcp_notice.clear();
                self.load_selected_mcp_server_oauth_status_task()
            }
            Message::CloseMcpServerEditor => {
                self.mcp_server_editor_open = false;
                Task::none()
            }
            Message::McpServerNameChanged(value) => {
                self.mcp_server_form.name = value;
                Task::none()
            }
            Message::McpServerTransportSelected(transport) => {
                self.mcp_server_form.transport = transport;
                Task::none()
            }
            Message::McpServerCommandChanged(value) => {
                self.mcp_server_form.command = value;
                Task::none()
            }
            Message::McpServerArgRowChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.args_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerArgRow => {
                self.mcp_server_form
                    .args_rows
                    .push(McpTextRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerArgRow(index) => {
                if self.mcp_server_form.args_rows.len() > 1
                    && index < self.mcp_server_form.args_rows.len()
                {
                    self.mcp_server_form.args_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerUrlChanged(value) => {
                self.mcp_server_form.url = value;
                Task::none()
            }
            Message::McpServerBearerTokenChanged(value) => {
                self.mcp_server_form.bearer_token = value;
                Task::none()
            }
            Message::McpServerHeaderKeyChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.header_rows.get_mut(index) {
                    row.key = value;
                }
                Task::none()
            }
            Message::McpServerHeaderValueChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.header_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerHeaderRow => {
                self.mcp_server_form
                    .header_rows
                    .push(McpKeyValueRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerHeaderRow(index) => {
                if self.mcp_server_form.header_rows.len() > 1
                    && index < self.mcp_server_form.header_rows.len()
                {
                    self.mcp_server_form.header_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerHeaderFromEnvKeyChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.header_from_env_rows.get_mut(index) {
                    row.key = value;
                }
                Task::none()
            }
            Message::McpServerHeaderFromEnvValueChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.header_from_env_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerHeaderFromEnvRow => {
                self.mcp_server_form
                    .header_from_env_rows
                    .push(McpKeyValueRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerHeaderFromEnvRow(index) => {
                if self.mcp_server_form.header_from_env_rows.len() > 1
                    && index < self.mcp_server_form.header_from_env_rows.len()
                {
                    self.mcp_server_form.header_from_env_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerOAuthClientIdChanged(value) => {
                self.mcp_server_form.oauth_client_id = value;
                Task::none()
            }
            Message::McpServerOAuthClientSecretChanged(value) => {
                self.mcp_server_form.oauth_client_secret = value;
                Task::none()
            }
            Message::McpServerOAuthScopesChanged(value) => {
                self.mcp_server_form.oauth_scopes_text = value;
                Task::none()
            }
            Message::McpServerOAuthRedirectUriChanged(value) => {
                self.mcp_server_form.oauth_redirect_uri = value;
                Task::none()
            }
            Message::McpServerCwdChanged(value) => {
                self.mcp_server_form.cwd = value;
                Task::none()
            }
            Message::McpServerEnvKeyChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.env_rows.get_mut(index) {
                    row.key = value;
                }
                Task::none()
            }
            Message::McpServerEnvValueChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.env_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerEnvRow => {
                self.mcp_server_form
                    .env_rows
                    .push(McpKeyValueRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerEnvRow(index) => {
                if self.mcp_server_form.env_rows.len() > 1
                    && index < self.mcp_server_form.env_rows.len()
                {
                    self.mcp_server_form.env_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerEnvPassthroughChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.env_passthrough_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerEnvPassthroughRow => {
                self.mcp_server_form
                    .env_passthrough_rows
                    .push(McpTextRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerEnvPassthroughRow(index) => {
                if self.mcp_server_form.env_passthrough_rows.len() > 1
                    && index < self.mcp_server_form.env_passthrough_rows.len()
                {
                    self.mcp_server_form.env_passthrough_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerOAuthCallbackInputChanged(value) => {
                self.mcp_oauth_callback_input = value;
                Task::none()
            }
            Message::McpServerEnabledToggled(enabled) => {
                self.mcp_server_form.enabled = enabled;
                Task::none()
            }
            Message::CreateMcpServer => {
                self.selected_mcp_server_id = None;
                self.mcp_server_editor_open = true;
                self.mcp_server_form = McpServerFormState {
                    enabled: true,
                    ..McpServerFormState::default()
                };
                self.mcp_oauth_status = String::from("未配置 OAuth");
                self.mcp_oauth_callback_input.clear();
                self.mcp_oauth_authorization_url.clear();
                self.mcp_oauth_pending_state = None;
                self.mcp_oauth_flow_in_flight = false;
                self.mcp_notice.clear();
                Task::none()
            }
            Message::SaveMcpServerConfiguration => {
                match self.save_selected_mcp_server_configuration() {
                    Ok(server_id) => {
                        self.selected_mcp_server_id = Some(server_id);
                        self.mcp_server_editor_open = false;
                        self.reload_selected_mcp_server_form();
                        self.mcp_oauth_callback_input.clear();
                        self.mcp_oauth_authorization_url.clear();
                        self.mcp_oauth_pending_state = None;
                        self.mcp_oauth_flow_in_flight = false;
                        self.mcp_notice = String::from("MCP 服务器配置已保存。");
                        self.status_message = String::from("MCP 服务器配置已写入本地文件。");
                        return self.restart_mcp_runtime_task();
                    }
                    Err(error) => {
                        self.mcp_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::RequestRemoveMcpServerConfiguration(server_id) => {
                let server_name = self
                    .provider_configuration
                    .mcp
                    .servers
                    .get(&server_id)
                    .map(|entry| entry.name.clone())
                    .unwrap_or_else(|| String::from("未命名服务器"));
                self.show_workbench_confirmation_modal(
                    format!("确认移除 MCP `{server_name}`"),
                    "移除后，该 MCP 服务器的本地配置会被立即删除；如果它正处于编辑状态，对应编辑器也会一起关闭。",
                    "确认移除",
                    "取消",
                    WorkbenchNoticeConfirmAction::RemoveMcpServerConfiguration { server_id },
                );
                Task::none()
            }
            Message::StartSelectedMcpServerOAuth => {
                let Some(server_id) = self.selected_mcp_server_id.clone() else {
                    self.mcp_notice = String::from("请先选择并保存一个支持远端连接的 MCP 服务器。");
                    self.status_message = self.mcp_notice.clone();
                    return Task::none();
                };
                let Some(entry) = self.selected_mcp_server_entry() else {
                    self.mcp_notice = String::from("当前选中的 MCP 服务器不存在，请先保存配置。");
                    self.status_message = self.mcp_notice.clone();
                    return Task::none();
                };
                if matches!(entry.transport, PersistedMcpTransportKind::Stdio) {
                    self.mcp_notice =
                        String::from("stdio 类型 MCP 服务器不需要宿主侧 OAuth 浏览器授权。");
                    self.status_message = self.mcp_notice.clone();
                    return Task::none();
                }
                if entry.oauth.is_none() {
                    self.mcp_notice = String::from(
                        "当前 MCP 服务器尚未配置 OAuth 信息，请先填写 redirect URI 或客户端参数并保存。",
                    );
                    self.status_message = self.mcp_notice.clone();
                    return Task::none();
                }
                self.mcp_oauth_flow_in_flight = true;
                self.mcp_notice = format!("正在为 MCP 服务器 `{server_id}` 生成 OAuth 授权链接。");
                self.status_message = self.mcp_notice.clone();
                self.start_selected_mcp_server_oauth_task()
            }
            Message::SelectedMcpServerOAuthChallengeReady(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                match result {
                    Ok((authorization_url, oauth_state)) => {
                        self.mcp_oauth_authorization_url = authorization_url.clone();
                        self.mcp_oauth_pending_state = Some(oauth_state.clone());
                        self.mcp_oauth_flow_in_flight = true;
                        self.mcp_notice = String::from(
                            "OAuth 授权链接已生成，正在尝试打开默认浏览器；若自动回调失败，可粘贴浏览器最终跳转 URL 或查询串完成授权。",
                        );
                        self.status_message = self.mcp_notice.clone();

                        let browser_task = Task::perform(
                            open_external_url_in_browser(authorization_url),
                            Message::McpServerOAuthBrowserOpened,
                        );
                        let callback_task = self
                            .selected_mcp_server_oauth_redirect_uri()
                            .and_then(|redirect_uri| {
                                resolve_loopback_oauth_redirect_port(&redirect_uri)
                            })
                            .map(|port| {
                                Task::perform(
                                    capture_loopback_oauth_callback(
                                        port,
                                        oauth_state,
                                        Duration::from_secs(300),
                                    ),
                                    move |callback_result| {
                                        Message::SelectedMcpServerOAuthCallbackCaptured(
                                            server_id.clone(),
                                            callback_result,
                                        )
                                    },
                                )
                            });

                        if let Some(callback_task) = callback_task {
                            Task::batch([browser_task, callback_task])
                        } else {
                            browser_task
                        }
                    }
                    Err(error) => {
                        self.mcp_oauth_flow_in_flight = false;
                        self.mcp_notice = format!("生成 MCP OAuth 授权链接失败：{error}");
                        self.status_message = self.mcp_notice.clone();
                        Task::none()
                    }
                }
            }
            Message::McpServerOAuthBrowserOpened(result) => {
                if let Err(error) = result {
                    self.mcp_notice = format!(
                        "已生成 OAuth 授权链接，但自动打开浏览器失败：{error}。请手动复制并打开下方链接。"
                    );
                    self.status_message = self.mcp_notice.clone();
                }
                Task::none()
            }
            Message::SelectedMcpServerOAuthCallbackCaptured(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                if self.mcp_oauth_pending_state.is_none() {
                    return Task::none();
                }
                match result {
                    Ok(callback_input) => {
                        self.mcp_oauth_callback_input = callback_input.clone();
                        let Ok(callback_payload) =
                            parse_hosted_oauth_callback_input(&callback_input)
                        else {
                            self.mcp_oauth_flow_in_flight = false;
                            self.mcp_notice = String::from(
                                "本地回调已捕获，但解析失败，请手动检查并重新发起授权。",
                            );
                            self.status_message = self.mcp_notice.clone();
                            return Task::none();
                        };
                        self.mcp_notice = String::from("已捕获本地 OAuth 回调，正在完成授权。");
                        self.status_message = self.mcp_notice.clone();
                        self.finish_selected_mcp_server_oauth_task(
                            callback_payload.authorization_code,
                            callback_payload.oauth_state,
                        )
                    }
                    Err(error) => {
                        self.mcp_notice = format!(
                            "等待本地 OAuth 回调失败：{error}。请改为粘贴浏览器最终跳转 URL 或查询串完成授权。"
                        );
                        self.status_message = self.mcp_notice.clone();
                        Task::none()
                    }
                }
            }
            Message::CompleteSelectedMcpServerOAuthFromInput => {
                let parsed = parse_hosted_oauth_callback_input(&self.mcp_oauth_callback_input);
                match parsed {
                    Ok(callback_payload) => {
                        if let Some(expected_state) = self.mcp_oauth_pending_state.as_deref() {
                            if callback_payload.oauth_state != expected_state {
                                self.mcp_notice = String::from(
                                    "手动回调中的 `state` 与当前等待值不一致，请重新发起授权。",
                                );
                                self.status_message = self.mcp_notice.clone();
                                self.mcp_oauth_flow_in_flight = false;
                                return Task::none();
                            }
                        }
                        self.mcp_notice = String::from("正在完成 MCP OAuth 授权。");
                        self.status_message = self.mcp_notice.clone();
                        self.finish_selected_mcp_server_oauth_task(
                            callback_payload.authorization_code,
                            callback_payload.oauth_state,
                        )
                    }
                    Err(error) => {
                        self.mcp_oauth_flow_in_flight = false;
                        self.mcp_notice = error.clone();
                        self.status_message = error;
                        Task::none()
                    }
                }
            }
            Message::SelectedMcpServerOAuthFinished(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                self.mcp_oauth_flow_in_flight = false;
                match result {
                    Ok(status_label) => {
                        self.mcp_oauth_status = status_label.clone();
                        self.mcp_notice = format!(
                            "MCP 服务器 `{server_id}` OAuth 授权已完成，当前状态：{status_label}。正在重启运行时并刷新连接。"
                        );
                        self.status_message = self.mcp_notice.clone();
                        self.mcp_oauth_pending_state = None;
                        return self.restart_mcp_runtime_task();
                    }
                    Err(error) => {
                        self.mcp_notice = format!("完成 MCP OAuth 授权失败：{error}");
                        self.status_message = self.mcp_notice.clone();
                    }
                }
                Task::none()
            }
            Message::RemoveSelectedMcpServerOAuthAuthorization => {
                self.mcp_notice = String::from("正在清理当前 MCP 服务器的 OAuth 凭据。");
                self.status_message = self.mcp_notice.clone();
                self.remove_selected_mcp_server_oauth_task()
            }
            Message::SelectedMcpServerOAuthAuthorizationRemoved(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                match result {
                    Ok(()) => {
                        self.mcp_oauth_status = String::from("未认证");
                        self.mcp_notice = format!(
                            "MCP 服务器 `{server_id}` 的 OAuth 凭据已清理，正在重启运行时。"
                        );
                        self.status_message = self.mcp_notice.clone();
                        self.mcp_oauth_pending_state = None;
                        self.mcp_oauth_authorization_url.clear();
                        self.mcp_oauth_callback_input.clear();
                        self.mcp_oauth_flow_in_flight = false;
                        return self.restart_mcp_runtime_task();
                    }
                    Err(error) => {
                        self.mcp_notice = format!("清理 MCP OAuth 凭据失败：{error}");
                        self.status_message = self.mcp_notice.clone();
                    }
                }
                Task::none()
            }
            Message::SelectedMcpServerOAuthStatusLoaded(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                self.mcp_oauth_status =
                    result.unwrap_or_else(|error| format!("状态读取失败：{error}"));
                Task::none()
            }
            _ => Task::none(),
        }
    }
}
