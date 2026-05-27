//! Blocking ask/question modal workflow extracted from the main workbench module.
//! 从主工作台模块中抽取的阻塞式 ask/question 弹窗流程。

use super::*;

impl VulcanWorkbench {
    /// Builds one fresh blocking ask/question modal state from a pending runtime request.
    /// 根据一条待处理运行时请求构建一份新的阻塞式 ask/question 弹窗状态。
    pub(super) fn build_question_modal_state(
        request: QuestionRequest,
    ) -> WorkbenchQuestionModalState {
        let drafts = request
            .questions
            .iter()
            .map(Self::build_question_modal_answer_draft)
            .collect::<Vec<_>>();
        let active_option_index = request
            .questions
            .first()
            .zip(drafts.first())
            .map(|(question, draft)| Self::default_question_modal_option_index(question, draft))
            .unwrap_or(0);
        WorkbenchQuestionModalState {
            request,
            origin: WorkbenchQuestionModalOrigin::Runtime,
            drafts,
            active_question_index: 0,
            active_option_index,
            stage: WorkbenchQuestionModalStage::Answering,
            validation_error: None,
        }
    }

    /// Builds one local ask debug modal state that enumerates representative question types for style tuning.
    /// 构建一份本地 ask 调试弹窗状态，枚举具有代表性的问题类型以便进行样式调试。
    pub(super) fn build_ask_debug_question_modal_state() -> WorkbenchQuestionModalState {
        let request = QuestionRequest {
            id: String::from("ask-debug-preview"),
            session_id: SessionId::nil(),
            sequence: 0,
            questions: Self::build_ask_debug_questions(),
        };
        let mut state = Self::build_question_modal_state(request);
        state.origin = WorkbenchQuestionModalOrigin::AskDebug;
        state
    }

    /// Builds one representative ask debug question list that covers single-choice, multi-choice, and free-text prompts.
    /// 构建一组具有代表性的 ask 调试题目，覆盖单选、多选与自由文本问题。
    pub(super) fn build_ask_debug_questions() -> Vec<QuestionPrompt> {
        vec![
            QuestionPrompt {
                question: String::from("Which overall density feels best for the ask modal?"),
                header: String::from("Density"),
                options: vec![
                    QuestionOption {
                        label: String::from("Compact"),
                        description: String::from(
                            "Tighter spacing and higher information density.",
                        ),
                    },
                    QuestionOption {
                        label: String::from("Balanced"),
                        description: String::from("Moderate spacing with clear grouping."),
                    },
                    QuestionOption {
                        label: String::from("Spacious"),
                        description: String::from("Larger spacing and more breathing room."),
                    },
                ],
                multiple: Some(false),
            },
            QuestionPrompt {
                question: String::from(
                    "Which interactions should remain visible in the modal itself?",
                ),
                header: String::from("Visible Hints"),
                options: vec![
                    QuestionOption {
                        label: String::from("Keyboard"),
                        description: String::from("Show key-driven interaction hints."),
                    },
                    QuestionOption {
                        label: String::from("Mouse"),
                        description: String::from("Show click-oriented helper text."),
                    },
                    QuestionOption {
                        label: String::from("Stage"),
                        description: String::from(
                            "Show progress like question count or confirm state.",
                        ),
                    },
                ],
                multiple: Some(true),
            },
            QuestionPrompt {
                question: String::from("What wording should the confirm page emphasize most?"),
                header: String::from("Confirm Copy"),
                options: vec![],
                multiple: Some(false),
            },
            QuestionPrompt {
                question: String::from(
                    "Which option row style reads best for long labels and descriptions?",
                ),
                header: String::from("Option Style"),
                options: vec![
                    QuestionOption {
                        label: String::from("Dense Row"),
                        description: String::from("Compact row with subdued description."),
                    },
                    QuestionOption {
                        label: String::from("Card Row"),
                        description: String::from(
                            "More obvious card separation and larger hit area.",
                        ),
                    },
                    QuestionOption {
                        label: String::from("Minimal"),
                        description: String::from(
                            "Primary label only unless extra detail is necessary.",
                        ),
                    },
                ],
                multiple: Some(false),
            },
        ]
    }

    /// Writes one Markdown ask debug reply preview into the dedicated example preview buffer so the style sandbox can inspect the final model-facing content.
    /// 将 Markdown 形式的 ask 调试回复预览写入独立示例预览缓冲区，以便样式沙盒检查最终面向模型的内容。
    pub(super) fn apply_ask_debug_answers_to_preview(
        &mut self,
        questions: &[QuestionPrompt],
        answers: &[Vec<String>],
    ) -> Result<(), String> {
        let preview_text = build_question_reply_markdown(questions, answers);
        self.example_preview_markdown_buffer = text_editor::Content::with_text(&preview_text);
        Ok(())
    }

    /// Builds one default answer draft for a single ask/question prompt while preserving the always-available custom-answer lane.
    /// 为单条 ask/question 提示构建默认答案草稿，并保留始终可用的“其他回答”通道。
    pub(super) fn build_question_modal_answer_draft(
        question: &QuestionPrompt,
    ) -> WorkbenchQuestionAnswerDraft {
        WorkbenchQuestionAnswerDraft {
            selected_labels: Vec::new(),
            custom_selected: question.options.is_empty(),
            custom_text: String::new(),
        }
    }

    /// Returns the active stage of the currently visible ask/question modal, if any.
    /// 返回当前可见 ask/question 弹窗的激活阶段（如果存在）。
    pub(super) fn active_question_modal_stage(&self) -> Option<WorkbenchQuestionModalStage> {
        self.active_question_modal.as_ref().map(|state| state.stage)
    }

    /// Returns the origin of the currently visible ask/question modal, if any.
    /// 返回当前可见 ask/question 弹窗的来源标记（如果存在）。
    pub(super) fn active_question_modal_origin(&self) -> Option<WorkbenchQuestionModalOrigin> {
        self.active_question_modal
            .as_ref()
            .map(|state| state.origin)
    }

    /// Returns whether one ask/question prompt should render the synthetic custom-answer row in addition to structured options.
    /// 返回某条 ask/question 提示是否应在结构化选项之外渲染合成的“其他回答”行。
    pub(super) fn question_modal_has_custom_option_row(question: &QuestionPrompt) -> bool {
        !question.options.is_empty()
    }

    /// Returns the rendered option count for one ask/question prompt, including the synthetic custom-answer row when it is visible.
    /// 返回单条 ask/question 提示的渲染选项数量，并在可见时包含合成的“其他回答”行。
    pub(super) fn question_modal_option_count(question: &QuestionPrompt) -> usize {
        question.options.len().saturating_add(usize::from(
            Self::question_modal_has_custom_option_row(question),
        ))
    }

    /// Returns the rendered option index occupied by the synthetic custom-answer row for one prompt.
    /// 返回某条提示中合成“其他回答”行所占据的渲染选项下标。
    pub(super) fn question_modal_custom_option_index(question: &QuestionPrompt) -> usize {
        question.options.len()
    }

    /// Returns the best default highlighted option index for one question draft when the editor lands on that question.
    /// 返回编辑器切换到某道题时该题草稿最合适的默认高亮选项下标。
    pub(super) fn default_question_modal_option_index(
        question: &QuestionPrompt,
        draft: &WorkbenchQuestionAnswerDraft,
    ) -> usize {
        if question.options.is_empty() {
            return 0;
        }
        if draft.custom_selected {
            return Self::question_modal_custom_option_index(question);
        }

        draft
            .selected_labels
            .iter()
            .find_map(|selected_label| {
                question
                    .options
                    .iter()
                    .position(|option| option.label == *selected_label)
            })
            .unwrap_or(0)
    }

    /// Reconciles one mirrored ask/question modal state with the latest runtime request shape while preserving in-progress drafts.
    /// 在保留进行中草稿的前提下，让镜像 ask/question 弹窗状态与最新运行时请求结构重新对齐。
    pub(super) fn reconcile_question_modal_state(state: &mut WorkbenchQuestionModalState) {
        let question_count = state.request.questions.len();

        if state.drafts.len() > question_count {
            state.drafts.truncate(question_count);
        } else {
            for question in state.request.questions.iter().skip(state.drafts.len()) {
                state
                    .drafts
                    .push(Self::build_question_modal_answer_draft(question));
            }
        }

        if question_count == 0 {
            state.active_question_index = 0;
            state.active_option_index = 0;
            state.stage = WorkbenchQuestionModalStage::Answering;
            return;
        }

        state.active_question_index = state
            .active_question_index
            .min(question_count.saturating_sub(1));
        let active_question = &state.request.questions[state.active_question_index];
        let max_option_index = Self::question_modal_option_count(active_question).saturating_sub(1);
        state.active_option_index = state.active_option_index.min(max_option_index);
    }

    /// Returns whether the current active ask/question page should focus the visible custom-answer text input.
    /// 返回当前激活 ask/question 页面是否应聚焦可见的自定义回答输入框。
    pub(super) fn active_question_modal_needs_text_input_focus(&self) -> bool {
        let Some(state) = self.active_question_modal.as_ref() else {
            return false;
        };
        if state.stage != WorkbenchQuestionModalStage::Answering {
            return false;
        }
        let Some((question, draft)) = state
            .request
            .questions
            .get(state.active_question_index)
            .zip(state.drafts.get(state.active_question_index))
        else {
            return false;
        };

        question.options.is_empty() || draft.custom_selected
    }

    /// Synchronizes keyboard focus so the active ask/question modal either owns the visible text input or clears background focus entirely.
    /// 同步键盘焦点，使当前 ask/question 弹窗要么占有可见文本输入框，要么彻底清空背景焦点。
    pub(super) fn sync_active_question_modal_focus(&self) -> Task<Message> {
        if self.active_question_modal_needs_text_input_focus() {
            operation::focus(WORKBENCH_QUESTION_MODAL_CUSTOM_INPUT_ID)
        } else {
            iced_runtime::task::effect(iced_runtime::Action::widget(unfocus()))
        }
    }

    /// Mirrors the oldest pending ask/question runtime request into the main workbench modal layer while preserving in-progress drafts for the currently visible request.
    /// 将最早的待回答 ask/question 运行时请求镜像到主工作台弹层，同时为当前可见请求保留进行中的答案草稿。
    pub(super) fn sync_active_question_modal_from_runtime(&mut self) -> bool {
        let pending_requests = match self.runtime_foundation.question_runtime().list_pending() {
            Ok(requests) => requests,
            Err(error) => {
                self.status_message = error;
                return false;
            }
        };
        let Some(next_request) = pending_requests.into_iter().next() else {
            if self.active_question_modal_origin() != Some(WorkbenchQuestionModalOrigin::AskDebug) {
                let had_modal = self.active_question_modal.is_some();
                self.active_question_modal = None;
                return had_modal;
            }
            return false;
        };
        if let Some(state) = self
            .active_question_modal
            .as_mut()
            .filter(|state| state.request.id == next_request.id)
        {
            state.request = next_request;
            Self::reconcile_question_modal_state(state);
            return false;
        }
        self.active_question_modal = Some(Self::build_question_modal_state(next_request));
        self.status_message = String::from("当前执行正在等待你回答补充问题。");
        true
    }

    /// Selects one question inside the active ask/question modal and restores the editing stage around it.
    /// 选中当前 ask/question 弹窗中的某个问题，并围绕它恢复编辑阶段。
    pub(super) fn select_active_question_modal_question(&mut self, question_index: usize) {
        let Some(state) = self.active_question_modal.as_mut() else {
            return;
        };
        if question_index >= state.request.questions.len() {
            return;
        }

        state.active_question_index = question_index;
        state.stage = WorkbenchQuestionModalStage::Answering;
        state.validation_error = None;
        if let Some((question, draft)) = state
            .request
            .questions
            .get(question_index)
            .zip(state.drafts.get(question_index))
        {
            state.active_option_index = Self::default_question_modal_option_index(question, draft);
        }
    }

    /// Moves the active ask/question modal editor to the previous or next question, using the dedicated confirmation page as the terminal tab.
    /// 将当前 ask/question 弹窗编辑器移动到上一题或下一题，并把专用确认页当作终点标签页。
    pub(super) fn move_active_question_modal_question(&mut self, forward: bool) {
        let Some(state) = self.active_question_modal.as_mut() else {
            return;
        };
        let question_count = state.request.questions.len();
        if question_count == 0 {
            return;
        }

        if forward {
            if state.stage == WorkbenchQuestionModalStage::Confirming {
                return;
            }
            if state.active_question_index + 1 < question_count {
                state.active_question_index += 1;
                state.stage = WorkbenchQuestionModalStage::Answering;
            } else {
                state.stage = WorkbenchQuestionModalStage::Confirming;
            }
        } else if state.stage == WorkbenchQuestionModalStage::Confirming {
            state.stage = WorkbenchQuestionModalStage::Answering;
            state.active_question_index = question_count.saturating_sub(1);
        } else if state.active_question_index > 0 {
            state.active_question_index = state.active_question_index.saturating_sub(1);
            state.stage = WorkbenchQuestionModalStage::Answering;
        } else {
            return;
        }

        state.validation_error = None;
        if state.stage == WorkbenchQuestionModalStage::Answering {
            if let Some((question, draft)) = state
                .request
                .questions
                .get(state.active_question_index)
                .zip(state.drafts.get(state.active_question_index))
            {
                state.active_option_index =
                    Self::default_question_modal_option_index(question, draft);
            }
        }
    }

    /// Moves the highlighted option within the active ask/question modal question with non-circular navigation.
    /// 在当前 ask/question 弹窗激活问题内以非循环方式移动高亮选项。
    pub(super) fn move_active_question_modal_option(&mut self, forward: bool) {
        let Some(state) = self.active_question_modal.as_mut() else {
            return;
        };
        if state.stage != WorkbenchQuestionModalStage::Answering {
            return;
        }
        let Some(question) = state.request.questions.get(state.active_question_index) else {
            return;
        };
        let option_count = Self::question_modal_option_count(question);
        if option_count == 0 {
            return;
        }

        if forward {
            state.active_option_index = state
                .active_option_index
                .saturating_add(1)
                .min(option_count.saturating_sub(1));
        } else {
            state.active_option_index = state.active_option_index.saturating_sub(1);
        }
        state.validation_error = None;
    }

    /// Toggles the currently highlighted option row inside the active ask/question modal question.
    /// 在当前 ask/question 弹窗激活问题中切换当前高亮选项行。
    pub(super) fn toggle_active_question_modal_highlighted_option(&mut self) {
        let Some((question_index, option_index)) = self
            .active_question_modal
            .as_ref()
            .map(|state| (state.active_question_index, state.active_option_index))
        else {
            return;
        };

        self.toggle_active_question_modal_option(question_index, option_index);
    }

    /// Toggles one structured or custom option row inside the currently visible ask/question modal draft state.
    /// 在当前可见 ask/question 弹窗草稿状态中切换一条结构化或自定义选项行。
    pub(super) fn toggle_active_question_modal_option(
        &mut self,
        question_index: usize,
        option_index: usize,
    ) {
        let Some(state) = self.active_question_modal.as_mut() else {
            return;
        };
        if question_index >= state.request.questions.len() || question_index >= state.drafts.len() {
            return;
        };
        state.active_question_index = question_index;
        state.active_option_index = option_index;
        state.stage = WorkbenchQuestionModalStage::Answering;
        state.validation_error = None;

        let question = &state.request.questions[question_index];
        if question.options.is_empty() {
            return;
        }
        let draft = &mut state.drafts[question_index];
        let custom_option_index = Self::question_modal_custom_option_index(question);
        let max_option_index = Self::question_modal_option_count(question).saturating_sub(1);
        state.active_option_index = state.active_option_index.min(max_option_index);

        if Self::question_modal_has_custom_option_row(question)
            && state.active_option_index == custom_option_index
        {
            if question.allows_multiple() {
                draft.custom_selected = !draft.custom_selected;
            } else {
                draft.selected_labels.clear();
                draft.custom_selected = true;
            }
            return;
        }

        let Some(option_label) = question
            .options
            .get(state.active_option_index)
            .map(|option| option.label.clone())
        else {
            return;
        };

        if question.allows_multiple() {
            if let Some(existing_index) = draft
                .selected_labels
                .iter()
                .position(|label| label == &option_label)
            {
                draft.selected_labels.remove(existing_index);
            } else {
                draft.selected_labels.push(option_label);
            }
            return;
        }

        draft.selected_labels = vec![option_label];
        draft.custom_selected = false;
    }

    /// Updates the custom free-text answer draft for one question inside the currently visible ask/question modal.
    /// 更新当前可见 ask/question 弹窗内某个问题的自定义自由文本答案草稿。
    pub(super) fn update_active_question_modal_custom_answer(
        &mut self,
        question_index: usize,
        value: String,
    ) {
        let Some(state) = self.active_question_modal.as_mut() else {
            return;
        };
        let Some(question) = state.request.questions.get(question_index) else {
            return;
        };
        let Some(draft) = state.drafts.get_mut(question_index) else {
            return;
        };
        state.active_question_index = question_index;
        state.active_option_index = Self::question_modal_custom_option_index(question);
        state.stage = WorkbenchQuestionModalStage::Answering;
        draft.custom_text = value;
        if question.options.is_empty() || !draft.custom_text.trim().is_empty() {
            draft.custom_selected = true;
        }
        state.validation_error = None;
    }

    /// Collects the normalized answer strings for one question draft or returns a user-facing validation error.
    /// 收集单题草稿的规范化答案字符串，若不完整则返回面向用户的校验错误。
    pub(super) fn collect_question_modal_prompt_answers(
        question: &QuestionPrompt,
        draft: &WorkbenchQuestionAnswerDraft,
    ) -> Result<Vec<String>, String> {
        let mut answers = draft.selected_labels.clone();
        if draft.custom_selected || question.options.is_empty() {
            let custom_answer = draft.custom_text.trim();
            if custom_answer.is_empty() {
                return if question.options.is_empty() {
                    Err(format!("请先填写“{}”的回答。", question.header))
                } else {
                    Err(format!("请先填写“{}”的其他回答。", question.header))
                };
            }
            answers.push(custom_answer.to_string());
        }

        if answers.is_empty() {
            return if question.options.is_empty() {
                Err(format!("请先填写“{}”的回答。", question.header))
            } else {
                Err(format!(
                    "请先为“{}”选择答案或填写其他回答。",
                    question.header
                ))
            };
        }

        Ok(answers)
    }

    /// Collects every pending answer from the active ask/question modal or returns the first validation failure with its question index.
    /// 收集当前 ask/question 弹窗中的全部待提交答案，或返回首个校验失败及其问题下标。
    pub(super) fn collect_active_question_modal_answers(
        &self,
    ) -> Result<Vec<Vec<String>>, (usize, String)> {
        let Some(state) = self.active_question_modal.as_ref() else {
            return Ok(Vec::new());
        };

        state
            .request
            .questions
            .iter()
            .enumerate()
            .map(|(question_index, question)| {
                let Some(draft) = state.drafts.get(question_index) else {
                    return Err((
                        question_index,
                        String::from("当前问题草稿状态不完整，请关闭后重试。"),
                    ));
                };
                Self::collect_question_modal_prompt_answers(question, draft)
                    .map_err(|error| (question_index, error))
            })
            .collect()
    }

    /// Collects every currently unanswered question inside the active ask/question modal so the confirmation page can link back to the missing items.
    /// 收集当前 ask/question 弹窗内所有尚未完成的问题，以便确认页回链到缺失项。
    pub(super) fn collect_active_question_modal_unanswered_questions(
        &self,
    ) -> Vec<(usize, String, String)> {
        let Some(state) = self.active_question_modal.as_ref() else {
            return Vec::new();
        };

        state
            .request
            .questions
            .iter()
            .enumerate()
            .filter_map(|(question_index, question)| {
                let draft = state.drafts.get(question_index)?;
                Self::collect_question_modal_prompt_answers(question, draft)
                    .err()
                    .map(|error| (question_index, question.header.clone(), error))
            })
            .collect()
    }

    /// Opens the dedicated confirmation page of the current ask/question modal without blocking incomplete drafts.
    /// 打开当前 ask/question 弹窗的专用确认页，而不会阻止未完成草稿先行查看。
    pub(super) fn open_active_question_modal_confirm_stage(&mut self) {
        let Some(state) = self.active_question_modal.as_mut() else {
            return;
        };
        state.stage = WorkbenchQuestionModalStage::Confirming;
        state.validation_error = None;
    }

    /// Opens one blocking confirmation modal before abandoning the active ask/question flow so accidental escape presses do not immediately interrupt the running conversation.
    /// 在放弃当前 ask/question 流程前打开一个阻塞确认弹窗，从而避免误触 Esc 直接中断正在执行的对话。
    pub(super) fn request_active_question_modal_rejection(&mut self) -> Task<Message> {
        let Some(origin) = self.active_question_modal_origin() else {
            return Task::none();
        };
        match origin {
            WorkbenchQuestionModalOrigin::Runtime => self.show_workbench_confirmation_modal(
                "放弃当前问题回答？",
                "放弃提交会导致当前对话执行立即中断，正在等待的 ask 工具调用会按失败结束。",
                "放弃提交",
                "继续回答",
                WorkbenchNoticeConfirmAction::AbandonQuestionModal,
            ),
            WorkbenchQuestionModalOrigin::AskDebug => self.show_workbench_confirmation_modal(
                "放弃当前 ask 调试？",
                "放弃提交后不会写入调试预览，当前 ask 调试窗口中的回答草稿也会一并丢失。",
                "放弃调试",
                "继续调整",
                WorkbenchNoticeConfirmAction::AbandonQuestionModal,
            ),
        }
        iced_runtime::task::effect(iced_runtime::Action::widget(unfocus()))
    }

    /// Confirms the current ask/question page, applying the highlighted single-choice row when needed and then moving to the next page or submitting from the confirmation page.
    /// 确认当前 ask/question 页面，在需要时应用当前高亮的单选行，然后进入下一页；若已在确认页则直接提交。
    pub(super) fn confirm_active_question_modal_page(&mut self) -> Task<Message> {
        let Some((stage, question_index, option_index, should_apply_highlighted_single_choice)) =
            self.active_question_modal.as_ref().map(|state| {
                let should_apply_highlighted_single_choice = state
                    .request
                    .questions
                    .get(state.active_question_index)
                    .is_some_and(|question| {
                        !question.options.is_empty() && !question.allows_multiple()
                    });
                (
                    state.stage,
                    state.active_question_index,
                    state.active_option_index,
                    should_apply_highlighted_single_choice,
                )
            })
        else {
            return Task::none();
        };

        if stage == WorkbenchQuestionModalStage::Confirming {
            return self.submit_active_question_modal();
        }

        if should_apply_highlighted_single_choice {
            self.toggle_active_question_modal_option(question_index, option_index);
        }
        self.move_active_question_modal_question(true);
        self.sync_active_question_modal_focus()
    }

    /// Validates and submits the currently visible ask/question modal back into the shared runtime so the blocked tool invocation can continue.
    /// 校验并提交当前可见 ask/question 弹窗到共享运行时，使被阻塞的工具调用继续执行。
    pub(super) fn submit_active_question_modal(&mut self) -> Task<Message> {
        if self.active_question_modal_stage() == Some(WorkbenchQuestionModalStage::Answering) {
            self.open_active_question_modal_confirm_stage();
            return self.sync_active_question_modal_focus();
        }

        let validation = self.collect_active_question_modal_answers();
        let (request_id, origin, questions, answers) = {
            let Some(state) = self.active_question_modal.as_mut() else {
                return Task::none();
            };
            match validation {
                Ok(answers) => {
                    state.validation_error = None;
                    (
                        state.request.id.clone(),
                        state.origin,
                        state.request.questions.clone(),
                        answers,
                    )
                }
                Err((_question_index, _error)) => {
                    state.stage = WorkbenchQuestionModalStage::Confirming;
                    state.validation_error = None;
                    return Task::none();
                }
            }
        };
        if origin == WorkbenchQuestionModalOrigin::AskDebug {
            match self.apply_ask_debug_answers_to_preview(&questions, &answers) {
                Ok(()) => {
                    self.active_question_modal = None;
                    self.status_message =
                        String::from("ask 调试回答已写入补丁与骨架预览，可继续调整样式。");
                }
                Err(error) => {
                    if let Some(state) = self.active_question_modal.as_mut() {
                        state.validation_error = Some(error);
                    }
                }
            }
            return Task::none();
        }
        match self
            .runtime_foundation
            .question_runtime()
            .reply(&request_id, answers)
        {
            Ok(_) => {
                self.active_question_modal = None;
                self.status_message = String::from("补充问题已提交，正在继续执行。");
            }
            Err(error) => {
                if let Some(state) = self.active_question_modal.as_mut() {
                    state.validation_error = Some(error);
                }
            }
        }
        Task::none()
    }

    /// Rejects the currently visible ask/question modal and propagates a cancellation error back into the waiting tool invocation.
    /// 拒绝当前可见 ask/question 弹窗，并把取消错误回传给等待中的工具调用。
    pub(super) fn reject_active_question_modal(&mut self) -> Task<Message> {
        let Some((request_id, origin)) = self
            .active_question_modal
            .as_ref()
            .map(|state| (state.request.id.clone(), state.origin))
        else {
            return Task::none();
        };
        if origin == WorkbenchQuestionModalOrigin::AskDebug {
            self.active_question_modal = None;
            self.status_message = String::from("已关闭 ask 调试弹窗。");
            return Task::none();
        }
        match self
            .runtime_foundation
            .question_runtime()
            .reject(&request_id)
        {
            Ok(_) => {
                self.active_question_modal = None;
                self.status_message = String::from("已放弃当前补充问题，本次对话执行已中断。");
            }
            Err(error) => {
                if let Some(state) = self.active_question_modal.as_mut() {
                    state.validation_error = Some(error);
                }
            }
        }
        Task::none()
    }
}
