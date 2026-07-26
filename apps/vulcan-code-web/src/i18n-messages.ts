// LocaleCode describes supported WebUI languages.
// LocaleCode 描述 WebUI 支持的语言。
export type LocaleCode = "zh-CN" | "en-US"

// TranslationKey describes text keys currently localized at the shell level.
// TranslationKey 描述当前在外壳层支持本地化的文本键。
export type TranslationKey =
  | "app.subtitle"
  | "notifications.appTitle"
  | "notifications.completed.generic"
  | "notifications.completed.withTitle"
  | "notifications.failed.generic"
  | "notifications.failed.withTitle"
  | "settings.notifications"
  | "settings.notifications.description"
  | "settings.notifications.supported"
  | "settings.notifications.unsupported"
  | "settings.notifications.localChannel"
  | "settings.notifications.localChannel.description"
  | "settings.notifications.permission"
  | "settings.notifications.permission.unsupported"
  | "settings.notifications.permission.default"
  | "settings.notifications.permission.granted"
  | "settings.notifications.permission.denied"
  | "settings.notifications.requestPermission"
  | "settings.notifications.requesting"
  | "settings.notifications.categories"
  | "settings.notifications.completed"
  | "settings.notifications.failed"
  | "settings.notifications.privacy"
  | "settings.notifications.privacy.description"
  | "settings.notifications.showTitle"
  | "settings.notifications.limitations"
  | "settings.notifications.browserLimitation"
  | "settings.notifications.desktopLimitation"
  | "app.serviceConnectionRestored"
  | "app.serviceReconnecting"
  | "app.serviceCheckingRecovery"
  | "area.chat"
  | "area.skills"
  | "area.agentSkills"
  | "area.agents"
  | "area.memory"
  | "area.knowledge"
  | "area.settings"
  | "area.chat.description"
  | "area.skills.description"
  | "area.agentSkills.description"
  | "area.agents.description"
  | "area.memory.description"
  | "area.knowledge.description"
  | "knowledge.name"
  | "knowledge.name.placeholder"
  | "knowledge.create"
  | "knowledge.filter.aria"
  | "knowledge.filter.placeholder"
  | "knowledge.refresh"
  | "knowledge.emptyLibraries"
  | "knowledge.libraryCounts"
  | "knowledge.defaultDescription"
  | "knowledge.saveName"
  | "knowledge.view"
  | "knowledge.importDirectory"
  | "knowledge.sources.title"
  | "knowledge.sources.description"
  | "knowledge.sources.add"
  | "knowledge.sources.loading"
  | "knowledge.sources.empty"
  | "knowledge.sources.documents"
  | "knowledge.sources.counts"
  | "knowledge.sources.lastScanned"
  | "knowledge.sources.sync"
  | "knowledge.sources.modify"
  | "knowledge.sources.modifyPickerTitle"
  | "knowledge.cancelImport"
  | "knowledge.cancelOperation"
  | "knowledge.repairIndex"
  | "knowledge.rebuildIndex"
  | "knowledge.indexHealth.title"
  | "knowledge.indexHealth.description"
  | "knowledge.indexHealth.loading"
  | "knowledge.indexHealth.searchable"
  | "knowledge.indexHealth.needsRepair"
  | "knowledge.indexHealth.authoritative"
  | "knowledge.indexHealth.indexed"
  | "knowledge.indexHealth.missing"
  | "knowledge.indexHealth.orphaned"
  | "knowledge.indexHealth.contentRevision"
  | "knowledge.indexHealth.dictionaryRevision"
  | "knowledge.indexHealth.tokenizer"
  | "knowledge.delete"
  | "knowledge.deleteConfirm"
  | "knowledge.projectAuthorization"
  | "knowledge.documentProject"
  | "knowledge.sessionDefaultEnabled"
  | "knowledge.currentSession"
  | "knowledge.selectionExplicit"
  | "knowledge.selectionInherited"
  | "knowledge.documents"
  | "knowledge.search.placeholder"
  | "knowledge.search"
  | "knowledge.searching"
  | "knowledge.search.empty"
  | "knowledge.viewer.maximize"
  | "knowledge.viewer.restore"
  | "knowledge.viewer.loadingDocuments"
  | "knowledge.viewer.loadingFolder"
  | "knowledge.viewer.loadMore"
  | "knowledge.viewer.loadingDocument"
  | "knowledge.viewer.selectDocument"
  | "knowledge.viewer.clearSearch"
  | "knowledge.viewer.searchCount"
  | "knowledge.search.substring"
  | "knowledge.body"
  | "knowledge.documentCounts"
  | "knowledge.emptyDocuments"
  | "knowledge.markdown.preview"
  | "knowledge.markdown.source"
  | "knowledge.previousPage"
  | "knowledge.page"
  | "knowledge.nextPage"
  | "knowledge.close"
  | "knowledge.emptySelection"
  | "knowledge.importConfirm"
  | "knowledge.pickerTitle"
  | "knowledge.nameRequired"
  | "knowledge.operationCompleted"
  | "knowledge.importProgress"
  | "knowledge.importCompleted"
  | "knowledge.importDialog.title"
  | "knowledge.operationDialog.title.import"
  | "knowledge.operationDialog.title.repair"
  | "knowledge.operationDialog.title.rebuild"
  | "knowledge.importDialog.preparing"
  | "knowledge.importDialog.scanning"
  | "knowledge.importDialog.processing"
  | "knowledge.importDialog.completed"
  | "knowledge.importDialog.cancelled"
  | "knowledge.importDialog.failed"
  | "knowledge.importDialog.discovered"
  | "knowledge.importDialog.total"
  | "knowledge.importDialog.bytes"
  | "knowledge.importDialog.indexItems"
  | "knowledge.importDialog.current"
  | "knowledge.importDialog.waiting"
  | "knowledge.importDialog.close"
  | "knowledge.importDialog.cancelling"
  | "knowledge.importDialog.cancellingDetail"
  | "knowledge.importDialog.committing"
  | "knowledge.importDialog.indexing"
  | "knowledge.importDialog.repairing"
  | "knowledge.importDialog.rebuilding"
  | "knowledge.importDialog.verifying"
  | "knowledge.importDialog.finalizing"
  | "knowledge.importDialog.committingFallback"
  | "knowledge.importDialog.indexingFallback"
  | "knowledge.importDialog.repairingFallback"
  | "knowledge.importDialog.rebuildingFallback"
  | "knowledge.importDialog.verifyingFallback"
  | "knowledge.importDialog.finalizingFallback"
  | "knowledge.importPhase.scanning"
  | "knowledge.importPhase.parsing"
  | "knowledge.importPhase.committing"
  | "knowledge.importPhase.indexing"
  | "knowledge.importPhase.repairing"
  | "knowledge.importPhase.rebuilding"
  | "knowledge.importPhase.verifying"
  | "knowledge.importPhase.finalizing"
  | "knowledge.importPhase.completed"
  | "knowledge.importPhase.cancelled"
  | "knowledge.composer.aria"
  | "knowledge.composer.label"
  | "knowledge.composer.manage"
  | "knowledge.composer.noneAuthorized"
  | "knowledge.package.import"
  | "knowledge.package.export"
  | "knowledge.package.exportPickerTitle"
  | "knowledge.package.importPickerTitle"
  | "knowledge.package.noPackages"
  | "knowledge.package.inspecting"
  | "knowledge.package.inspect"
  | "knowledge.package.importTitle"
  | "knowledge.package.counts"
  | "knowledge.package.nameConflict"
  | "knowledge.package.createStrategy"
  | "knowledge.package.renameRequired"
  | "knowledge.package.createDescription"
  | "knowledge.package.replaceStrategy"
  | "knowledge.package.replaceDescription"
  | "knowledge.package.replaceConfirm"
  | "knowledge.package.exportCompleted"
  | "knowledge.package.importCompleted"
  | "knowledge.package.replaceCompleted"
  | "area.settings.description"
  | "feature.status.available"
  | "feature.status.preview"
  | "global.openSettings"
  | "global.toggleDark"
  | "global.toggleLight"
  | "global.language"
  | "global.languageSaveFailed"
  | "global.online"
  | "global.preview"
  | "global.progress"
  | "global.labelSeparator"
  | "global.listDelimiter"
  | "appChrome.menu"
  | "appChrome.status"
  | "appChrome.theme"
  | "appChrome.theme.dark"
  | "appChrome.theme.light"
  | "appChrome.language.zh"
  | "appChrome.scrollFeaturesLeft"
  | "appChrome.scrollFeaturesRight"
  | "appChrome.windowActionFailed"
  | "appChrome.window.minimize"
  | "appChrome.window.restore"
  | "appChrome.window.maximize"
  | "appChrome.window.close"
  | "appChrome.githubOpenFailed"
  | "appChrome.githubAria"
  | "sidebar.user.local"
  | "sidebar.user.account"
  | "sidebar.user.logout"
  | "projectMenu.conversationsAria"
  | "projectMenu.newConversation"
  | "projectMenu.noConversations"
  | "projectMenu.pin"
  | "projectMenu.unpin"
  | "projectMenu.openDirectory"
  | "projectMenu.createWorktree"
  | "projectMenu.renameProject"
  | "projectMenu.setIcon"
  | "projectMenu.archiveConversations"
  | "projectMenu.removeProject"
  | "projectMenu.projectSettings"
  | "conversationMenu.pin"
  | "conversationMenu.unpin"
  | "conversationMenu.rename"
  | "conversationMenu.archive"
  | "conversationMenu.markUnread"
  | "conversationMenu.openDirectory"
  | "conversationMenu.copyTitle"
  | "conversationMenu.copyWorkspace"
  | "conversationMenu.copySessionId"
  | "conversationMenu.copyDeepLink"
  | "conversationMenu.forkLocal"
  | "conversationMenu.forkWorktree"
  | "conversationMenu.openNewWindow"
  | "conversationMenu.moreTitleActions"
  | "diffAction.moreStaging"
  | "diffAction.unstageAll"
  | "diffAction.unstageFile"
  | "diffAction.restoreAll"
  | "diffAction.stageAll"
  | "diffAction.stageFile"
  | "diffAction.restoreFile"
  | "diff.mode.unstaged"
  | "diff.mode.staged"
  | "diff.mode.commit"
  | "diff.mode.branch"
  | "diff.branch.current"
  | "diff.branch.main"
  | "diff.view.toUnified"
  | "diff.view.toSplit"
  | "diff.refresh"
  | "diff.loading"
  | "diff.moreActions"
  | "diff.copyRaw"
  | "diff.inline.disable"
  | "diff.inline.enable"
  | "diff.wrap.disable"
  | "diff.wrap.enable"
  | "diff.contextControls.hide"
  | "diff.contextControls.show"
  | "diff.indicator.useClassic"
  | "diff.indicator.useBars"
  | "diff.search.open"
  | "diff.search.label"
  | "diff.search.truncated"
  | "diff.search.previous"
  | "diff.search.next"
  | "diff.search.indexUnavailable"
  | "diff.commit.none"
  | "diff.branch.search"
  | "diff.branch.label"
  | "diff.branch.noMatches"
  | "diff.emptyBody"
  | "diff.noMatchingFiles"
  | "diff.file.tooLarge"
  | "diff.file.added"
  | "diff.file.deleted"
  | "diff.file.gitlink"
  | "diff.file.renamedNoChanges"
  | "diff.file.noContent"
  | "diff.render.changedLinesSkipped"
  | "diff.render.skipped"
  | "diff.render.rowsLightweight"
  | "diff.large.totalFiles"
  | "diff.large.totalChangedLines"
  | "diff.large.totalChangedBytes"
  | "diff.large.fileChangedLines"
  | "diff.large.fileChangedBytes"
  | "diff.large.fileMaxLineBytes"
  | "diff.load.rowsPending"
  | "diff.load.retry"
  | "diff.load.reveal"
  | "diff.load.matchCount"
  | "diff.load.backendMatchFallback"
  | "diff.load.loadingDetail"
  | "diff.load.timeout"
  | "diff.load.failed"
  | "diff.load.loadedEmpty"
  | "diff.load.summaryIdle"
  | "console.title"
  | "console.newConversation"
  | "console.emptyPrompt"
  | "conversation.loading.title"
  | "conversation.loading.requesting"
  | "conversation.loading.locating"
  | "conversation.loading.index"
  | "conversation.loading.reading"
  | "conversation.loading.subagents"
  | "conversation.loading.projecting"
  | "conversation.loading.done"
  | "conversation.loading.failed"
  | "conversation.unlinkedProject"
  | "conversation.history.loadMore"
  | "conversation.history.loading"
  | "conversation.action.copyFinal"
  | "conversation.action.copiedFinal"
  | "conversation.action.fork"
  | "conversation.action.forkTitleSuffix"
  | "conversation.action.forkFailed"
  | "conversation.action.forkSucceeded"
  | "conversation.action.forkInProgressTitle"
  | "conversation.action.forkInProgressMessage"
  | "conversation.forkSource.label"
  | "conversation.forkSource.missing"
  | "conversation.block.toolCall"
  | "conversation.block.toolResult"
  | "conversation.block.command"
  | "conversation.block.thinking"
  | "conversation.block.plan"
  | "conversation.block.notice"
  | "conversation.block.detail"
  | "conversation.metadata.type"
  | "conversation.metadata.status"
  | "conversation.metadata.tool"
  | "conversation.metadata.identifier"
  | "conversation.metadata.error"
  | "conversation.metadata.result"
  | "conversation.metadata.returned"
  | "conversation.toolRequest.receiving"
  | "conversation.toolRequest.ready"
  | "conversation.toolRequest.characters"
  | "conversation.stats.providerCalls"
  | "conversation.stats.toolCalls"
  | "conversation.stats.totalTokens"
  | "conversation.stats.cachedTokens"
  | "conversation.stats.cachedTokensCompact"
  | "conversation.stats.outputSpeed"
  | "conversation.stats.group.regular"
  | "conversation.stats.group.subagent"
  | "conversation.stats.group.contextCompaction"
  | "conversation.subagent.stopTask"
  | "conversation.stopWithAgents.title"
  | "conversation.stopWithAgents.description"
  | "conversation.stopWithAgents.mainOnly"
  | "conversation.stopWithAgents.all"
  | "conversation.process.header"
  | "conversation.process.stoppedAfter"
  | "conversation.process.stopped"
  | "conversation.process.duration"
  | "conversation.toolSequence.summary"
  | "conversation.toolSequence.runningSummary"
  | "conversation.action.editPrompt"
  | "conversation.action.editPromptFailed"
  | "conversation.action.cancel"
  | "conversation.action.saveAndReplay"
  | "contextCompaction.title.running"
  | "contextCompaction.title.completed"
  | "contextCompaction.title.failed"
  | "contextCompaction.generating"
  | "contextCompaction.thinkingLabel"
  | "contextCompaction.thinkingActive"
  | "contextCompaction.emptyThinking"
  | "contextCompaction.failed.trigger"
  | "contextCompaction.failed.error"
  | "contextCompaction.failed.note"
  | "contextCompaction.trigger.manual"
  | "contextCompaction.trigger.auto"
  | "contextCompaction.progress.prepareInput"
  | "contextCompaction.progress.buildRequest"
  | "contextCompaction.progress.prepareRuntime"
  | "contextCompaction.progress.runtimeReady"
  | "contextCompaction.progress.prepared"
  | "contextCompaction.progress.requestReady"
  | "contextCompaction.progress.providerCheckpoint"
  | "contextCompaction.progress.providerLoweringRepair"
  | "contextCompaction.progress.wireRequestReady"
  | "contextCompaction.progress.requestStarted"
  | "contextCompaction.progress.thinkingDelta"
  | "contextCompaction.progress.outputDelta"
  | "contextCompaction.progress.prepareExecution"
  | "contextCompaction.progress.executionReady"
  | "contextCompaction.progress.bootstrapRuntime"
  | "contextCompaction.progress.unknown"
  | "contextPanel.open"
  | "contextPanel.close"
  | "contextPanel.review"
  | "contextPanel.memory"
  | "contextPanel.files"
  | "contextPanel.dynamic"
  | "contextPanel.static"
  | "contextPanel.reviewEmpty"
  | "contextPanel.memoryEmpty"
  | "contextPanel.dynamicEmpty"
  | "contextPanel.staticEmpty"
  | "contextUsage.title"
  | "contextUsage.waiting"
  | "sidebar.console"
  | "sidebar.pinned"
  | "sidebar.projects"
  | "sidebar.unlinkedConversations"
  | "sidebar.addProject"
  | "sidebar.noProjects"
  | "sidebar.waitingWorkspace"
  | "sidebar.overview"
  | "sidebar.settingsCategories"
  | "sidebar.toggle"
  | "composer.mode.execute"
  | "composer.mode.plan"
  | "composer.capability.extended"
  | "composer.capability.extended.subtitle"
  | "composer.capability.basic"
  | "composer.capability.basic.subtitle"
  | "composer.capability.enabledCount"
  | "composer.capability.tooltipTitle"
  | "composer.capability.tooltipEmpty"
  | "composer.capability.dialogTitle"
  | "composer.capability.dialogDescription"
  | "composer.capability.source.nativeWithTargets"
  | "composer.capability.source.native"
  | "composer.capability.source.targets"
  | "composer.capability.source.noTargets"
  | "composer.capability.empty.native"
  | "composer.capability.empty.noTargets"
  | "composer.capability.saveHint.externalUpdate"
  | "composer.capability.saveHint.saving"
  | "composer.capability.saveHint.unsaved"
  | "composer.capability.saveHint.clean"
  | "composer.capability.useDefaults"
  | "composer.capability.save.saving"
  | "composer.capability.save.saved"
  | "composer.capability.save"
  | "composer.capability.close.title"
  | "composer.capability.close.description"
  | "composer.capability.close.discard"
  | "composer.capability.close.keepEditing"
  | "composer.reasoning.off"
  | "composer.reasoning.low"
  | "composer.reasoning.medium"
  | "composer.reasoning.high"
  | "composer.reasoning.max"
  | "composer.reasoning.subtitle"
  | "composer.placeholder"
  | "composer.shortcutHint"
  | "composer.selectFallback"
  | "composer.aria.input"
  | "composer.aria.mode"
  | "composer.aria.model"
  | "composer.aria.capability"
  | "composer.aria.reasoning"
  | "composer.aria.send"
  | "composer.aria.stop"
  | "composer.modelPicker.filter"
  | "composer.modelPicker.noMatches"
  | "composer.attachment.add"
  | "composer.attachment.extension"
  | "composer.attachment.menu"
  | "composer.attachment.empty"
  | "composer.attachment.remove"
  | "composer.attachment.unsupported"
  | "composer.attachment.maxLimit"
  | "composer.attachment.partialMaxLimit"
  | "composer.attachment.kind.image"
  | "composer.attachment.kind.audio"
  | "composer.attachment.kind.video"
  | "composer.attachment.kind.pdf"
  | "composer.thinkingProcess"
  | "composer.planMode"
  | "composer.vmmMemory"
  | "composer.vmm.precheckInjection"
  | "composer.vmm.memoryWriteback"
  | "composer.vmm.mode.inheritProject"
  | "composer.vmm.mode.enabled"
  | "composer.vmm.mode.disabled"
  | "composer.goal"
  | "composer.scrollToBottom"
  | "composer.goalDialog.resumeTitle"
  | "composer.goalDialog.editTitle"
  | "composer.goalDialog.objectiveAria"
  | "composer.goalDialog.objectivePlaceholder"
  | "composer.goalDialog.tokenBudgetAria"
  | "composer.goalDialog.tokenBudgetPlaceholder"
  | "composer.goalDialog.save"
  | "composer.goal.status.active"
  | "composer.goal.status.paused"
  | "composer.goal.status.blocked"
  | "composer.goal.status.usageLimited"
  | "composer.goal.status.budgetLimited"
  | "composer.goal.status.completed"
  | "composer.goal.tooltip.input"
  | "composer.goal.tooltip.cache"
  | "composer.goal.tooltip.output"
  | "composer.goal.tooltip.reasoning"
  | "composer.goal.tooltip.total"
  | "composer.goal.action.edit"
  | "composer.goal.action.resumeTask"
  | "composer.goal.action.resume"
  | "composer.goal.action.pause"
  | "composer.goal.action.clear"
  | "composer.goal.error.empty"
  | "composer.goal.error.budgetInvalid"
  | "composer.planMode.label"
  | "composer.planMode.goalConflict"
  | "composer.error.stopCurrentFailed"
  | "composer.error.promptEmpty"
  | "composer.modelPicker.tooltipTitle"
  | "composer.modelPicker.currentModel"
  | "composer.modelPicker.currentProvider"
  | "composer.secondaryRoute.select"
  | "composer.secondaryRoute.search"
  | "composer.secondaryRoute.autoEndpoint"
  | "composer.secondaryRoute.loading"
  | "composer.secondaryRoute.noMatches"
  | "composer.secondaryRoute.switchFailed"
  | "composer.secondaryRoute.switched"
  | "composer.secondaryRoute.restoredAuto"
  | "composer.progress.open"
  | "composer.progress.progressLabel"
  | "composer.progress.step"
  | "composer.progress.changedFiles"
  | "composer.progress.stepInProgress"
  | "composer.reasoning.tooltip"
  | "pendingPrompt.queue"
  | "pendingPrompt.drag"
  | "pendingPrompt.delete"
  | "pendingPrompt.edit"
  | "pendingPrompt.insert"
  | "pendingPrompt.inserted"
  | "pendingPrompt.insertShort"
  | "pendingPrompt.insertedShort"
  | "pendingPrompt.attachmentCount"
  | "pendingPrompt.attachmentMessage"
  | "sessionActions.noProjectForCapability"
  | "sessionActions.noConversationForCapability"
  | "sessionActions.previewCannotSaveCapability"
  | "sessionActions.capabilitySaved"
  | "sessionActions.noConversationForGoalSave"
  | "sessionActions.previewCannotSaveGoal"
  | "sessionActions.goalSaved"
  | "sessionActions.noExistingConversation"
  | "sessionActions.previewCannotResumeGoal"
  | "sessionActions.goalResumed"
  | "sessionActions.previewCannotClearGoal"
  | "sessionActions.goalCleared"
  | "sessionActions.previewCannotUpdateGoal"
  | "sessionActions.noConversationForPlanMode"
  | "sessionActions.previewCannotTogglePlanMode"
  | "sessionActions.planModeEnabled"
  | "sessionActions.planModeDisabled"
  | "sessionActions.noConversationForVmmPolicy"
  | "sessionActions.previewCannotSetVmmPolicy"
  | "sessionActions.vmmPolicySaved"
  | "conversationActions.noProjectForSend"
  | "conversationActions.previewCannotSend"
  | "conversationActions.previewCannotCompact"
  | "conversationActions.modelSwitched"
  | "projectActions.vmmSaved"
  | "projectActions.openRequested"
  | "projectActions.unpinned"
  | "projectActions.pinned"
  | "projectActions.iconUpdated"
  | "projectActions.iconReset"
  | "projectActions.listUpdated"
  | "projectActions.skillInherited"
  | "projectActions.skillEnabled"
  | "projectActions.skillDisabled"
  | "projectSettings.cleanupCompleted"
  | "projectDialog.renameProjectTitle"
  | "projectDialog.renameConversationTitle"
  | "projectDialog.renameProjectDescription"
  | "projectDialog.renameConversationDescription"
  | "projectDialog.renamePlaceholder"
  | "projectDialog.confirm"
  | "projectDialog.archiveRemoveTitle"
  | "projectDialog.archiveTitle"
  | "projectDialog.archiveRemoveDescription"
  | "projectDialog.archiveDescription"
  | "projectDialog.currentProject"
  | "projectDialog.archiveAndRemove"
  | "projectDialog.archiveAll"
  | "projectDialog.removing"
  | "projectDialog.archiving"
  | "projectRow.conversationMenuAria"
  | "projectRow.createConversationAria"
  | "projectRow.collapseConversations"
  | "projectRow.showMoreConversations"
  | "projectRow.noConversations"
  | "conversationRow.running"
  | "conversationRow.unread"
  | "conversationRow.askAttention"
  | "conversationRow.planAttention"
  | "conversationRow.confirmArchive"
  | "projectFolderPicker.title"
  | "projectFolderPicker.pathAria"
  | "projectFolderPicker.openPath"
  | "projectFolderPicker.parent"
  | "projectFolderPicker.rootsAria"
  | "projectFolderPicker.noRoots"
  | "projectFolderPicker.hidden"
  | "projectFolderPicker.noChildren"
  | "projectFolderPicker.adding"
  | "projectIcon.title"
  | "projectIcon.description"
  | "projectIcon.previewAria"
  | "projectIcon.customTab"
  | "projectIcon.filterPlaceholder"
  | "projectIcon.loadMore"
  | "projectIcon.text"
  | "projectIcon.textPlaceholder"
  | "projectIcon.shape"
  | "projectIcon.shapeCircle"
  | "projectIcon.shapeSquare"
  | "projectIcon.backgroundColor"
  | "projectIcon.borderColor"
  | "projectIcon.foregroundColor"
  | "projectIcon.resetDefault"
  | "projectIcon.set"
  | "projectSettings.dialogTitle"
  | "projectSettings.dialogDescription"
  | "projectSettings.categoryAria"
  | "projectSettings.overview"
  | "projectSettings.pinnedProject"
  | "projectSettings.projectSpaceSummary"
  | "projectSettings.vmmMemory"
  | "projectSettings.knowledgeBases"
  | "projectSettings.knowledgeBasesDetail"
  | "projectSettings.knowledgeBasesDescription"
  | "projectSettings.knowledgeBasesLoading"
  | "projectSettings.knowledgeBasesSaved"
  | "projectSettings.projectOverride"
  | "projectSettings.inheritGlobal"
  | "projectSettings.promptSkillCount"
  | "projectSettings.attachmentCache"
  | "projectSettings.attachmentCleanup"
  | "projectSettings.projectSpace"
  | "projectSettings.projectId"
  | "projectSettings.conversationCount"
  | "projectSettings.pinStatus"
  | "projectSettings.pinned"
  | "projectSettings.notPinned"
  | "projectSettings.vmmHasOverride"
  | "projectSettings.vmmInheritsGlobal"
  | "projectSettings.agentOverrideCount"
  | "projectSettings.agentInheritsGlobal"
  | "projectSettings.vmmTitle"
  | "projectSettings.codebaseMemory.title"
  | "projectSettings.codebaseMemory.description"
  | "projectSettings.codebaseMemory.enabled"
  | "projectSettings.codebaseMemory.disabled"
  | "projectSettings.codebaseMemory.loading"
  | "projectSettings.codebaseMemory.enable"
  | "projectSettings.codebaseMemory.enableHint"
  | "projectSettings.codebaseMemory.disableHint"
  | "projectSettings.codebaseMemory.serviceStatus"
  | "projectSettings.codebaseMemory.indexStatus"
  | "projectSettings.codebaseMemory.lastIndexed"
  | "projectSettings.codebaseMemory.revision"
  | "projectSettings.codebaseMemory.unavailable"
  | "projectSettings.codebaseMemory.reindex"
  | "projectSettings.codebaseMemory.enabledSaved"
  | "projectSettings.codebaseMemory.disabledSaved"
  | "projectSettings.codebaseMemory.reindexAccepted"
  | "projectSettings.vmmDescription"
  | "projectSettings.vmmLoading"
  | "projectSettings.enableProjectOverride"
  | "projectSettings.overrideEnabledHint"
  | "projectSettings.overrideDisabledHint"
  | "projectSettings.vmmTransportHint"
  | "projectSettings.saveVmm"
  | "projectSettings.agentTitle"
  | "projectSettings.agentDescription"
  | "projectSettings.agent.globalEnabled"
  | "projectSettings.agent.globalDisabled"
  | "projectSettings.agentLoading"
  | "projectSettings.agentEmpty"
  | "projectSettings.agent.projectEnabled"
  | "projectSettings.agent.projectDisabled"
  | "projectSettings.agent.inheritEnabled"
  | "projectSettings.agent.inheritDisabled"
  | "projectSettings.agent.optionInheritEnabled"
  | "projectSettings.agent.optionInheritDisabled"
  | "projectSettings.agent.projectStateAria"
  | "projectSettings.cleanupTitle"
  | "projectSettings.cleanupDescription"
  | "projectSettings.managedDirectory"
  | "projectSettings.cutoffDate"
  | "projectSettings.attachmentKind.image"
  | "projectSettings.attachmentKind.audio"
  | "projectSettings.attachmentKind.video"
  | "projectSettings.cleanup.previewing"
  | "projectSettings.cleanup.preview"
  | "projectSettings.cleanup.cleaning"
  | "projectSettings.cleanup.execute"
  | "projectSettings.lastPreview"
  | "projectSettings.lastExecution"
  | "projectSettings.cleanup.notExecuted"
  | "projectSettings.cleanup.previewFallback"
  | "projectSettings.cleanup.invalidDate"
  | "projectSettings.cleanup.selectKind"
  | "projectSettings.cleanup.initialNotice"
  | "shell.windowOperationFailed"
  | "shell.openGithubFailed"
  | "shell.minimizeWindow"
  | "shell.maximizeWindow"
  | "shell.restoreWindow"
  | "shell.closeWindow"
  | "shell.openRepository"
  | "startup.loadingProjectIndex"
  | "startup.connectingService"
  | "startup.loadingStep"
  | "startup.loadingWorkspace"
  | "startup.downloadProgress"
  | "startup.failed"
  | "workspaceFiles.copyContentSuccess"
  | "workspaceFiles.noProjectDirectory"
  | "workspaceFiles.noMatchedFiles"
  | "workspaceFiles.noDisplayableFiles"
  | "workspaceFiles.filterPlaceholder"
  | "workspaceFiles.loading"
  | "workspaceFiles.loadingTree"
  | "workspaceFiles.filterRules"
  | "workspaceFiles.hiddenRules"
  | "workspaceFiles.ignoreHiddenFiles"
  | "workspaceFiles.ignoreDotNames"
  | "workspaceFiles.useIgnoreRules"
  | "workspaceFiles.useGitignoreRules"
  | "workspaceFiles.filterNonPreviewFiles"
  | "workspaceFiles.fileActions"
  | "workspaceFiles.copyPath"
  | "workspaceFiles.copyRelativePath"
  | "workspaceFiles.copyFileContent"
  | "workspaceFiles.filterFiles"
  | "workspaceFilePreview.openFile"
  | "workspaceFilePreview.emptyHint"
  | "workspaceFilePreview.loading"
  | "workspaceFilePreview.unsupportedTitle"
  | "workspaceFilePreview.unsupportedHint"
  | "workspaceFilePreview.noContent"
  | "workspaceFilePreview.missingTitle"
  | "workspaceFilePreview.video"
  | "workspaceFilePreview.image"
  | "workspaceFilePreview.mediaBlockedTitle"
  | "workspaceFilePreview.mediaBlockedHint"
  | "workspaceFilePreview.allowMedia"
  | "workspaceFilePreview.truncated"
  | "workspaceFilePreview.missingError"
  | "workspaceFileTree.collapseDirectory"
  | "workspaceFileTree.expandDirectory"
  | "workspaceFileTree.expandAll"
  | "workspaceFileTree.collapseAll"
  | "workspaceToolPanel.expandState"
  | "workspaceToolPanel.memorySettingsMenu"
  | "workspaceToolPanel.skillsSettingsMenu"
  | "workspaceToolPanel.agentsSettingsMenu"
  | "workspaceToolPanel.menu"
  | "workspaceToolPanel.collapseToPill"
  | "workspaceToolPanel.status"
  | "workspaceShell.skillsTitle"
  | "workspaceShell.agentsTitle"
  | "workspaceShell.memoryTitle"
  | "workspaceShell.version"
  | "workspacePanel.resizeRight"
  | "workspaceContext.switchTabs"
  | "workspaceContext.searchTabs"
  | "workspaceContext.searchTabsPlaceholder"
  | "workspaceContext.clearTabSearch"
  | "workspaceContext.openTabs"
  | "workspaceContext.closeTab"
  | "workspaceContext.openRightPage"
  | "workspaceContext.openPage"
  | "workspaceContext.launcher.filesDescription"
  | "workspaceContext.launcher.reviewDescription"
  | "workspaceContext.launcher.memoryDescription"
  | "workspaceContext.loadingReviewPanel"
  | "workspaceContext.loadingFilesPanel"
  | "workspaceMemory.handleDynamicMemory"
  | "workspaceMemory.expiredRegionTitle"
  | "workspaceMemory.expiredRegionDescription"
  | "workspaceMemory.suppressForSession"
  | "workspaceMemory.permanentDeleteInvalid"
  | "workspaceMemory.viewDetails"
  | "workspaceMemory.turnId"
  | "workspaceMemory.loadingTurn"
  | "workspaceMemory.lookupFailed"
  | "workspaceMemory.user"
  | "workspaceMemory.assistant"
  | "workspaceMemory.viewDetailsWithTurn"
  | "workspaceMemory.handleMemoryWithId"
  | "workspaceMemory.suppressedNotice"
  | "workspaceMemory.suppressFailed"
  | "workspaceMemory.deletedNotice"
  | "workspaceMemory.deletedWithoutId"
  | "workspaceMemory.deleteFailed"
  | "workspaceReview.refreshGitDiffFailed"
  | "workspaceReview.noProjectPathForGit"
  | "workspaceReview.gitOperationFailed"
  | "workspaceReview.fileDetailTimeout"
  | "workspaceReview.fileDetailFailed"
  | "workspaceReview.noProjectPathForRefresh"
  | "workspaceReview.fileDetailLoading"
  | "workspaceReview.hunkExpanding"
  | "workspaceReview.hunkDirectionUp"
  | "workspaceReview.hunkDirectionDown"
  | "workspaceReview.hunkDirectionBoth"
  | "workspaceReview.hunkExpandingSpecific"
  | "workspaceReview.fileDetailTimeoutError"
  | "workbenchConversation.unpinned"
  | "workbenchConversation.pinned"
  | "workbenchConversation.archived"
  | "workbenchConversation.noProject"
  | "workbenchConversation.copiedWorkspace"
  | "workbenchConversation.copiedTitle"
  | "workbenchConversation.copiedSessionId"
  | "workbenchConversation.nameEmpty"
  | "workbenchConversation.projectRenamed"
  | "workbenchConversation.projectNameRestored"
  | "workbenchConversation.conversationRenamed"
  | "workbenchConversation.archivedCount"
  | "workbenchConversation.archivedAndRemovedProject"
  | "workbenchAsk.submitted"
  | "workbenchAsk.cancelled"
  | "thinkingDisplay.popupOnly"
  | "thinkingDisplay.inlineOutput"
  | "thinkingDisplay.firstInlineWithPopup"
  | "thinkingDisplay.firstInlineOnly"
  | "ask.title"
  | "ask.description"
  | "ask.action.confirmSubmit"
  | "ask.action.confirmReject"
  | "ask.action.submit"
  | "ask.action.continue"
  | "ask.action.ignore"
  | "ask.action.backToEdit"
  | "ask.action.processing"
  | "ask.progress.previous"
  | "ask.progress.next"
  | "ask.option.selected"
  | "ask.confirm.submitTitle"
  | "ask.confirm.rejectTitle"
  | "ask.confirm.submitBody"
  | "ask.confirm.rejectBody"
  | "ask.custom.textPlaceholder"
  | "ask.custom.adjustCodex"
  | "ask.custom.description"
  | "ask.validation.requiredAnswer"
  | "ask.validation.requiredOther"
  | "ask.validation.requiredSelectionOrOther"
  | "terminalStop.title"
  | "terminalStop.description"
  | "terminalStop.stopping"
  | "terminalStop.confirm"
  | "conversation.bodyLoadFailed"
  | "conversation.emptyLoaded"
  | "conversation.emptyWaiting"
  | "conversation.detail.copy"
  | "conversation.detail.close"
  | "conversation.userPrompt.expand"
  | "conversation.userPrompt.collapse"
  | "conversation.userPrompt.copy"
  | "conversation.userPrompt.edit"
  | "conversation.subagent.schedule"
  | "conversation.subagent.call"
  | "conversation.subagent.calling"
  | "conversation.subagent.closeTask"
  | "conversation.subagent.loadingTranscript"
  | "conversation.subagent.closeConfirmTitle"
  | "conversation.subagent.closeConfirmDescription"
  | "conversation.subagent.closeConfirm"
  | "conversation.subagent.closing"
  | "conversation.subagent.state.running"
  | "conversation.subagent.state.stopped"
  | "conversation.subagent.state.failed"
  | "conversation.subagent.state.completed"
  | "conversation.subagent.detail.createdTask"
  | "conversation.subagent.detail.name"
  | "conversation.subagent.detail.description"
  | "conversation.subagent.detail.prompt"
  | "conversation.subagent.detail.effectivePrompt"
  | "conversation.subagent.detail.debug"
  | "conversation.subagent.detail.empty"
  | "conversation.subagent.detail.message"
  | "conversation.subagent.detail.wait"
  | "conversation.subagent.detail.waitPending"
  | "conversation.subagent.detail.waitCompleted"
  | "conversation.subagent.detail.waitInterrupted"
  | "conversation.subagent.detail.waitTimedOut"
  | "conversation.subagent.detail.templateCollection"
  | "conversation.subagent.detail.statusCollection"
  | "conversation.subagent.detail.feedback"
  | "conversation.subagent.detail.openFeedback"
  | "conversation.subagent.detail.count"
  | "conversation.subagent.detail.completed"
  | "conversation.subagent.detail.timeout"
  | "conversation.subagent.detail.noAgents"
  | "conversation.subagent.detail.noOpenAgents"
  | "conversation.subagent.enabled"
  | "conversation.subagent.disabled"
  | "conversation.subagent.detail.agent"
  | "conversation.subagent.detail.status"
  | "conversation.subagent.detail.unknown"
  | "conversation.subagent.detail.result"
  | "conversation.subagent.detail.noResult"
  | "conversation.subagent.detail.target"
  | "conversation.subagent.detail.queued"
  | "conversation.subagent.detail.deliveryMode"
  | "conversation.subagent.detail.targetSessions"
  | "conversation.subagent.detail.note"
  | "conversation.subagent.detail.closeControl"
  | "conversation.subagent.detail.interruptControl"
  | "conversation.subagent.detail.stoppedCount"
  | "conversation.subagent.detail.stoppedSessions"
  | "conversation.subagent.detail.closedCount"
  | "conversation.subagent.detail.closedAgents"
  | "conversation.subagent.detail.clearedMessages"
  | "conversation.subagent.detail.yes"
  | "conversation.subagent.detail.no"
  | "conversation.subagent.stopConfirmTitle"
  | "conversation.subagent.stopConfirmDescription"
  | "conversation.subagent.stopConfirm"
  | "conversation.subagent.stopping"
  | "conversation.diff.loadingPanel"
  | "conversation.diff.parsingWorker"
  | "conversation.diff.parseFailed"
  | "conversation.diff.unstructuredFallback"
  | "sessionTools.expand"
  | "sessionTools.collapse"
  | "sessionTools.changes"
  | "sessionTools.progress"
  | "sessionTools.terminal"
  | "sessionTools.menu"
  | "terminalSession.openScreen"
  | "terminalSession.noScreen"
  | "terminalSession.forceStop"
  | "terminalSession.forceStopSession"
  | "terminalSession.emptyScreen"
  | "terminalSession.tooltip.status"
  | "terminalSession.tooltip.command"
  | "terminalSession.tooltip.cwd"
  | "terminalSession.tooltip.shell"
  | "terminalSession.tooltip.backend"
  | "terminalSession.tooltip.screenFrame"
  | "terminalSession.tooltip.exitCode"
  | "terminalSession.tooltip.error"
  | "terminalSession.tooltip.output"
  | "terminalSession.tooltip.session"
  | "terminalState.running"
  | "terminalState.waiting"
  | "terminalState.success"
  | "terminalState.failed"
  | "terminalState.stopped"
  | "terminalState.lost"
  | "terminalState.unknown"
  | "toolPresentation.output.loading"
  | "toolPresentation.output.empty"
  | "toolPresentation.exitCode"
  | "toolPresentation.command.running"
  | "toolPresentation.command.failed"
  | "toolPresentation.command.stopped"
  | "toolPresentation.command.completed"
  | "toolPresentation.command.start.failed"
  | "toolPresentation.command.read.failed"
  | "toolPresentation.command.send.failed"
  | "toolPresentation.command.write.failed"
  | "toolPresentation.command.stop.failed"
  | "toolPresentation.command.start.running"
  | "toolPresentation.command.read.running"
  | "toolPresentation.command.send.running"
  | "toolPresentation.command.write.running"
  | "toolPresentation.command.stop.running"
  | "toolPresentation.command.start.completed"
  | "toolPresentation.command.list.completed"
  | "toolPresentation.command.read.completed"
  | "toolPresentation.command.send.completed"
  | "toolPresentation.command.write.completed"
  | "toolPresentation.command.stop.completed"
  | "toolPresentation.command.heading.start"
  | "toolPresentation.command.heading.read"
  | "toolPresentation.command.heading.send"
  | "toolPresentation.command.heading.write"
  | "toolPresentation.command.heading.stop"
  | "toolPresentation.command.session"
  | "toolPresentation.command.lost"
  | "toolPresentation.command.current"
  | "toolPresentation.command.currentState"
  | "toolPresentation.lua.title"
  | "toolPresentation.lua.running"
  | "toolPresentation.lua.failed"
  | "toolPresentation.lua.stopped"
  | "toolPresentation.lua.completed"
  | "toolPresentation.lua.action.eval"
  | "toolPresentation.lua.action.invalid"
  | "toolPresentation.lua.action.list"
  | "toolPresentation.lua.action.start"
  | "toolPresentation.lua.action.status"
  | "toolPresentation.lua.action.close"
  | "toolPresentation.lua.oneShot"
  | "toolPresentation.lua.persistent"
  | "toolPresentation.lua.session"
  | "toolPresentation.lua.ttl"
  | "toolPresentation.lua.elapsed"
  | "toolPresentation.lua.code"
  | "toolPresentation.lua.arguments"
  | "toolPresentation.lua.output"
  | "toolPresentation.lua.argumentName"
  | "toolPresentation.lua.argumentValue"
  | "toolPresentation.lua.sessionName"
  | "toolPresentation.lua.sessionId"
  | "toolPresentation.lua.sessionState"
  | "toolPresentation.lua.sessionTtl"
  | "toolPresentation.lua.sessionExpiresAt"
  | "toolPresentation.lua.sessionActive"
  | "toolPresentation.lua.sessionClosed"
  | "toolPresentation.lua.sessionExpired"
  | "toolPresentation.lua.sessionFailed"
  | "toolPresentation.lua.noSessions"
  | "toolPresentation.lua.closeSucceeded"
  | "toolPresentation.ask.running"
  | "toolPresentation.ask.ignored"
  | "toolPresentation.ask.completed"
  | "toolPresentation.ask.questionLabel"
  | "toolPresentation.ask.answerLabel"
  | "toolPresentation.ask.waitingAnswer"
  | "toolPresentation.file.count"
  | "toolPresentation.file.multiChange"
  | "toolPresentation.file.empty"
  | "toolPresentation.file.create.running"
  | "toolPresentation.file.modify.running"
  | "toolPresentation.file.delete.running"
  | "toolPresentation.file.mixed.running"
  | "toolPresentation.file.create.failed"
  | "toolPresentation.file.modify.failed"
  | "toolPresentation.file.delete.failed"
  | "toolPresentation.file.mixed.failed"
  | "toolPresentation.file.create.preview"
  | "toolPresentation.file.modify.preview"
  | "toolPresentation.file.delete.preview"
  | "toolPresentation.file.mixed.preview"
  | "toolPresentation.file.create.completed"
  | "toolPresentation.file.modify.completed"
  | "toolPresentation.file.delete.completed"
  | "toolPresentation.file.mixed.completed"
  | "toolPresentation.markdownOutput.running"
  | "toolPresentation.markdownOutput.failed"
  | "toolPresentation.markdownOutput.stopped"
  | "toolPresentation.markdownOutput.completed"
  | "toolPresentation.fileList.title"
  | "toolPresentation.fileRead.title"
  | "toolPresentation.codekit.astTree.title"
  | "toolPresentation.codekit.astDetail.title"
  | "toolPresentation.codekit.rg.title"
  | "toolPresentation.codekit.nodeSource.title"
  | "markdown.wrap.enable"
  | "markdown.wrap.disable"
  | "markdown.copyCode"
  | "markdown.copied"
  | "mediaPlayer.maximize"
  | "mediaPlayer.restore"
  | "mediaPlayer.close"
  | "mediaPlayer.fitWindow"
  | "mediaPlayer.originalSize"
  | "mediaPlayer.zoomOut"
  | "mediaPlayer.zoomIn"
  | "media.openVideoPlayer"
  | "settings.backToWorkspace"
  | "memory.scope"
  | "memory.scope.global"
  | "memory.scope.project"
  | "memory.project"
  | "memory.action.testConnection"
  | "memory.action.refreshUsersProjects"
  | "memory.vmm.action.running"
  | "memory.vmm.action.failed"
  | "memory.vmm.project.loading"
  | "memory.vmm.project.selected"
  | "memory.vmm.save.global.running"
  | "memory.vmm.save.project.running"
  | "memory.vmm.save.global.completed"
  | "memory.vmm.save.project.completed"
  | "memory.vmm.save.failed"
  | "vmm.common.optional"
  | "vmm.common.number"
  | "vmm.common.enabled"
  | "vmm.common.disabled"
  | "vmm.common.none"
  | "vmm.common.notConfigured"
  | "vmm.common.notSelected"
  | "vmm.common.running"
  | "vmm.overview.effectiveConfig"
  | "vmm.overview.grpc"
  | "vmm.overview.recallPolicy"
  | "vmm.overview.ttlTurns"
  | "vmm.overview.precheckAiTools"
  | "vmm.overview.writeback"
  | "vmm.overview.runtime"
  | "vmm.overview.outboxProfiles"
  | "vmm.overview.completedTurns"
  | "vmm.overview.summaryEntry"
  | "vmm.overview.noSummary"
  | "vmm.runtime.title"
  | "vmm.runtime.subtitle"
  | "vmm.runtime.session"
  | "vmm.runtime.profileNodes"
  | "vmm.runtime.updatedAt"
  | "vmm.runtime.noProfileBase"
  | "vmm.result.title"
  | "vmm.result.emptySummary"
  | "vmm.result.request"
  | "vmm.result.response"
  | "vmm.result.emptyHint"
  | "vmm.result.generic"
  | "vmm.settings.grpcTarget"
  | "vmm.settings.apiKey"
  | "vmm.settings.receiveTimeout"
  | "vmm.settings.handshakeTimeout"
  | "vmm.settings.maxReceiveBytes"
  | "vmm.settings.keepaliveInterval"
  | "vmm.settings.keepaliveTimeout"
  | "vmm.settings.idleKeepalive"
  | "vmm.settings.enableMemoryConnection"
  | "vmm.settings.exposeAiTools"
  | "vmm.settings.autoPrecheck"
  | "vmm.settings.precheckPolicy"
  | "vmm.settings.writebackPolicy"
  | "vmm.settings.policy.inherit"
  | "vmm.settings.policy.enabled"
  | "vmm.settings.policy.disabled"
  | "vmm.settings.userId"
  | "vmm.settings.projectId"
  | "vmm.settings.implicitMemoryTtl"
  | "vmm.settings.profileRefreshInterval"
  | "vmm.settings.profileNodeLimit"
  | "vmm.settings.injectionTag"
  | "vmm.settings.injectionDescription"
  | "vmm.rows.usersEmpty"
  | "vmm.rows.projectsEmpty"
  | "vmm.rows.profileNodesEmpty"
  | "vmm.rows.bundleEmpty"
  | "vmm.rows.toolResultEmpty"
  | "vmm.rows.unnamedUser"
  | "vmm.rows.unnamedProject"
  | "vmm.rows.deleted"
  | "vmm.rows.notFound"
  | "vmm.rows.vectorRows"
  | "vmm.profile.full"
  | "vmm.profile.team"
  | "vmm.profile.space"
  | "vmm.profile.project"
  | "vmm.profile.user"
  | "vmm.action.deleteMemory"
  | "vmm.action.precheckProject"
  | "vmm.action.confirmCreate"
  | "vmm.action.deleteProject"
  | "vmm.action.runMigrate"
  | "vmm.action.refreshNodes"
  | "vmm.action.applyInstruction"
  | "vmm.action.loadProfileBundle"
  | "vmm.action.saveGlobal"
  | "vmm.action.saveProject"
  | "vmm.action.resolveUser"
  | "vmm.action.deleteUser"
  | "vmm.section.project.title"
  | "vmm.section.project.subtitle"
  | "vmm.section.profile.title"
  | "vmm.section.profile.subtitle"
  | "vmm.section.bundle.title"
  | "vmm.section.bundle.subtitle"
  | "vmm.section.tool.title"
  | "vmm.section.tool.subtitle"
  | "vmm.section.memory.title"
  | "vmm.section.memory.subtitle"
  | "vmm.section.transport.title"
  | "vmm.section.transport.subtitle"
  | "vmm.section.user.title"
  | "vmm.section.user.subtitle"
  | "vmm.field.projectPath"
  | "vmm.field.confirmDelete"
  | "vmm.field.migrateSource"
  | "vmm.field.migrateTarget"
  | "vmm.field.confirmMigrate"
  | "vmm.field.profileTarget"
  | "vmm.field.profileInstruction"
  | "vmm.field.searchQueries"
  | "vmm.field.deleteReason"
  | "vmm.field.writeAbstract"
  | "vmm.field.writeDetails"
  | "vmm.field.userRef"
  | "vmm.field.deleteConfirmationCode"
  | "vmm.placeholder.profileInstruction"
  | "vmm.placeholder.multiPipe"
  | "vmm.placeholder.commaOrPipe"
  | "vmm.placeholder.deleteReason"
  | "vmm.placeholder.writeAbstract"
  | "vmm.placeholder.writeDetails"
  | "vmm.placeholder.userRef"
  | "vmm.placeholder.deleteConfirmationCode"
  | "vmm.diagnostic.disabledByConfig"
  | "vmm.diagnostic.missingGrpcTarget"
  | "vmm.diagnostic.grpcOffline"
  | "vmm.diagnostic.missingUserId"
  | "vmm.diagnostic.missingProjectId"
  | "planArtifact.titleFallback"
  | "planArtifact.copy"
  | "planArtifact.copied"
  | "planArtifact.expand"
  | "planArtifact.collapse"
  | "planArtifact.confirmAria"
  | "planArtifact.noteAria"
  | "planArtifact.notePlaceholder"
  | "planArtifact.execute"
  | "planArtifact.executeWithNote"
  | "planArtifact.defer"
  | "planArtifact.suggest"
  | "runtime.memoryRecall"
  | "runtime.requesting"
  | "runtime.toolCalling"
  | "runtime.thinking"
  | "runtime.thinkingOverlayDockRight"
  | "runtime.thinkingOverlayRestoreCentered"
  | "settings.general"
  | "settings.systemSettings"
  | "settings.projects"
  | "settings.providers"
  | "settings.tools"
  | "settings.diagnostics"
  | "settings.general.description"
  | "settings.projects.description"
  | "settings.providers.description"
  | "settings.tools.description"
  | "settings.diagnostics.description"
  | "settings.group.models"
  | "settings.group.aiCapabilities"
  | "settings.modelProviders"
  | "settings.modelList"
  | "settings.providerRetry"
  | "settings.aiVendors"
  | "settings.aiWebSearch"
  | "settings.aiUnderstandImage"
  | "settings.aiUnderstandVideo"
  | "settings.aiUnderstandAudio"
  | "settings.aiImageGeneration"
  | "settings.aiVideoGeneration"
  | "settings.aiSpeechSynthesis"
  | "settings.aiMusicGeneration"
  | "settings.aiMusicCover"
  | "settings.mcpServers"
  | "settings.runtimeDiagnostics"
  | "settings.gitCommit"
  | "settings.promptComposer"
  | "settings.aboutLicenses"
  | "settings.archivedRestore"
  | "settings.modelProviders.description"
  | "settings.modelList.description"
  | "settings.providerRetry.description"
  | "settings.aiVendors.description"
  | "settings.aiWebSearch.description"
  | "settings.aiUnderstandImage.description"
  | "settings.aiUnderstandVideo.description"
  | "settings.aiUnderstandAudio.description"
  | "settings.aiImageGeneration.description"
  | "settings.aiVideoGeneration.description"
  | "settings.aiSpeechSynthesis.description"
  | "settings.aiMusicGeneration.description"
  | "settings.aiMusicCover.description"
  | "settings.mcpServers.description"
  | "settings.runtimeDiagnostics.description"
  | "settings.diagnostics.logs"
  | "settings.diagnostics.startupStatus"
  | "settings.diagnostics.waitingStatus"
  | "settings.diagnostics.runtimeModules"
  | "settings.gitCommit.description"
  | "settings.gitCommit.active"
  | "settings.gitCommit.autoSummary"
  | "settings.gitCommit.summaryModel"
  | "settings.gitCommit.followSessionModel"
  | "settings.gitCommit.chooseSummaryModel"
  | "settings.gitCommit.resetSummaryModel"
  | "settings.gitCommit.followSessionModelHelp"
  | "settings.gitCommit.fixedSummaryModelHelp"
  | "settings.gitCommit.fixedSummaryModelUnavailable"
  | "settings.gitCommit.summaryExtraPrompt"
  | "settings.gitCommit.summaryExtraPromptPlaceholder"
  | "settings.gitCommit.summaryExtraPromptHelp"
  | "gitCommitDialog.generateEmpty"
  | "gitCommitDialog.generated"
  | "gitCommitDialog.generatedTruncated"
  | "gitCommitDialog.actionDone"
  | "gitCommitDialog.pushDone"
  | "gitCommitDialog.generating"
  | "gitCommitDialog.preparingCommitContent"
  | "gitCommitDialog.generatingCommitMessage"
  | "gitCommitDialog.committing"
  | "gitCommitDialog.pushingDetailed"
  | "gitCommitDialog.running"
  | "gitCommitDialog.pushing"
  | "gitCommitDialog.toolsTitle"
  | "gitCommitDialog.changes"
  | "gitCommitDialog.open"
  | "gitCommitDialog.title"
  | "gitCommitDialog.description"
  | "gitCommitDialog.currentBranch"
  | "gitCommitDialog.newBranch"
  | "gitCommitDialog.recommendedPrefix"
  | "gitCommitDialog.noChanges"
  | "gitCommitDialog.branchName"
  | "gitCommitDialog.message"
  | "gitCommitDialog.messagePlaceholder"
  | "gitCommitDialog.includeUnstaged"
  | "gitCommitDialog.commit"
  | "gitCommitDialog.commitAndPush"
  | "gitCommitDialog.push"
  | "gitCommitDialog.commitTo"
  | "settings.promptComposer.description"
  | "settings.aboutLicenses.description"
  | "settings.archivedRestore.description"
  | "skills.action.refresh"
  | "skills.action.refreshing"
  | "skills.action.installLua"
  | "skills.action.install"
  | "skills.action.installing"
  | "skills.action.cancel"
  | "skills.action.details"
  | "skills.action.tools"
  | "skills.action.action"
  | "skills.action.enable"
  | "skills.action.disable"
  | "skills.action.update"
  | "skills.action.delete"
  | "skills.status.enabled"
  | "skills.status.stopped"
  | "skills.filter.searchLabel"
  | "skills.filter.searchPlaceholder"
  | "skills.filter.statusLabel"
  | "skills.filter.allStatuses"
  | "skills.filter.issues"
  | "skills.filter.sourceLabel"
  | "skills.filter.allSources"
  | "skills.filter.empty"
  | "skills.label.system"
  | "skills.label.user"
  | "skills.label.source"
  | "skills.label.explicit"
  | "skills.label.implicit"
  | "skills.label.closed"
  | "skills.label.currentStatus"
  | "skills.label.installLayer"
  | "skills.label.installManagement"
  | "skills.label.runtimeEntries"
  | "skills.label.runtimeTools"
  | "skills.label.managedInstall"
  | "skills.label.unmanagedInstall"
  | "skills.label.scope"
  | "skills.label.skillFile"
  | "skills.label.skillDir"
  | "skills.source.unknown"
  | "skills.sidebar.luaDetail"
  | "skills.navigation.list"
  | "skills.navigation.plugins"
  | "skills.navigation.rtkSettings"
  | "skills.navigation.luaSettings"
  | "skills.luaSettings.description"
  | "skills.luaSettings.restartRequired"
  | "skills.luaSettings.restartHint"
  | "skills.luaSettings.saved"
  | "skills.luaSettings.save"
  | "skills.luaSettings.saving"
  | "skills.luaSettings.aiTools"
  | "skills.luaSettings.aiToolsHint"
  | "skills.luaSettings.enableRunLua"
  | "skills.luaSettings.enableRunLuaSession"
  | "skills.luaSettings.runLuaTooltip"
  | "skills.luaSettings.runLuaSessionTooltip"
  | "skills.luaSettings.mainPool"
  | "skills.luaSettings.runLuaPool"
  | "skills.luaSettings.poolHint"
  | "skills.luaSettings.minSize"
  | "skills.luaSettings.maxSize"
  | "skills.luaSettings.idleTtl"
  | "skills.luaSettings.toolCache"
  | "skills.luaSettings.maxEntries"
  | "skills.luaSettings.defaultTtl"
  | "skills.luaSettings.maxTtl"
  | "skills.luaSettings.network"
  | "skills.luaSettings.allowDownloads"
  | "skills.luaSettings.githubBaseUrl"
  | "skills.luaSettings.githubApiBaseUrl"
  | "skills.luaSettings.skillHubBaseUrl"
  | "skills.luaSettings.privateUrlInstall"
  | "skills.luaSettings.privateAllowlist"
  | "skills.luaSettings.privateAllowlistHint"
  | "skills.luaSettings.capabilities"
  | "skills.luaSettings.defaultEncoding"
  | "skills.luaSettings.autoEncoding"
  | "skills.luaSettings.skillManagementBridge"
  | "skills.luaSettings.managedIoCompat"
  | "skills.luaSettings.managedRuntime"
  | "skills.luaSettings.managedRuntimeHint"
  | "skills.luaSettings.managedRuntimeStatusReady"
  | "skills.luaSettings.managedRuntimeStatusInstalled"
  | "skills.luaSettings.managedRuntimeStatusUnsupported"
  | "skills.luaSettings.managedRuntimeStatusUnavailable"
  | "skills.luaSettings.managedRuntimeRetrying"
  | "skills.luaSettings.managedRuntimeRetryCompleted"
  | "skills.luaSettings.pythonRuntime"
  | "skills.luaSettings.nodeRuntime"
  | "skills.luaSettings.managedRuntimeInstalled"
  | "skills.luaSettings.managedRuntimeReinstall"
  | "skills.luaSettings.managedRuntimePreparing"
  | "skills.luaSettings.combinedRuntime"
  | "skills.luaSettings.managedRuntimeNormal"
  | "skills.luaSettings.managedWorkerPoolMax"
  | "skills.luaSettings.managedWorkerIdleTtl"
  | "skills.luaSettings.managedSessionLimit"
  | "skills.luaSettings.managedSessionBuffer"
  | "skills.luaSettings.managedInvokeTimeout"
  | "skills.luaSettings.unlimited"
  | "skills.luaSettings.validationPositive"
  | "skills.luaSettings.validationPoolOrder"
  | "skills.luaSettings.validationCacheOrder"
  | "skills.sidebar.agentDetail"
  | "skills.sidebar.pluginDetail"
  | "skills.header.luaDetail"
  | "skills.header.pluginDetail"
  | "skills.header.rtkDetail"
  | "agents.header.detail"
  | "skills.lua.emptyTitle"
  | "skills.lua.emptyDetail"
  | "skills.lua.installTitle"
  | "skills.lua.installDescription"
  | "skills.lua.sourceType"
  | "skills.lua.installSource"
  | "skills.lua.placeholderGithub"
  | "skills.lua.placeholderUrl"
  | "skills.lua.detailsTitle"
  | "skills.lua.detailsDescription"
  | "skills.lua.toolsTitle"
  | "skills.lua.toolsDescription"
  | "skills.lua.toolsEmpty"
  | "skills.lua.toolModeAria"
  | "skills.lua.toolExplicit"
  | "skills.lua.toolImplicit"
  | "skills.lua.toolDisabled"
  | "skills.lua.syncing"
  | "skills.luaConfig.title"
  | "skills.luaConfig.description"
  | "skills.luaConfig.loading"
  | "skills.luaConfig.empty"
  | "skills.luaConfig.complete"
  | "skills.luaConfig.incomplete"
  | "skills.luaConfig.required"
  | "skills.luaConfig.sensitive"
  | "skills.luaConfig.restartRequired"
  | "skills.luaConfig.deprecated"
  | "skills.luaConfig.secretConfigured"
  | "skills.luaConfig.preserveSecret"
  | "skills.luaConfig.selectValue"
  | "skills.luaConfig.deleteValue"
  | "skills.luaConfig.reload"
  | "skills.luaConfig.advanced"
  | "skills.luaConfig.orphaned"
  | "skills.luaConfig.externalChange"
  | "skills.luaConfig.revisionConflict"
  | "skills.luaConfig.saving"
  | "skills.luaConfig.save"
  | "skills.plugins.emptyTitle"
  | "skills.plugins.emptyDetail"
  | "skills.plugins.loadingPage"
  | "skills.plugins.externalUrlConfirmation"
  | "skills.plugins.externalUrlOpenAnyway"
  | "skills.plugins.externalUrlOpenFailed"
  | "skills.plugins.unavailable"
  | "skills.plugins.enabling"
  | "skills.plugins.draining"
  | "skills.plugins.cleanupWarning"
  | "skills.plugins.unsupportedPlatform"
  | "skills.plugins.retryCleanup"
  | "skills.plugins.systemKind"
  | "skills.plugins.tools"
  | "skills.plugins.hooks"
  | "skills.plugins.sessions"
  | "skills.plugins.diagnostics"
  | "skills.agent.emptyTitle"
  | "skills.agent.emptyDetail"
  | "skills.agent.detailsTitle"
  | "skills.agent.detailsDescription"

// translations stores shell-level localized labels.
// translations 保存外壳层本地化标签。
export const translations: Record<LocaleCode, Record<TranslationKey, string>> = {
  "zh-CN": {
    "app.subtitle": "Ai Coding",
    "notifications.appTitle": "Vulcan Code",
    "notifications.completed.generic": "会话已完成",
    "notifications.completed.withTitle": "会话“{conversationTitle}”已完成",
    "notifications.failed.generic": "会话执行失败",
    "notifications.failed.withTitle": "会话“{conversationTitle}”执行失败",
    "settings.notifications": "通知",
    "settings.notifications.description": "管理当前客户端的会话完成通知、权限与隐私。",
    "settings.notifications.supported": "当前壳支持",
    "settings.notifications.unsupported": "当前壳不支持",
    "settings.notifications.localChannel": "本地系统通知",
    "settings.notifications.localChannel.description": "只影响当前浏览器或桌面客户端，不会同步到账号或其他设备。",
    "settings.notifications.permission": "权限",
    "settings.notifications.permission.unsupported": "不支持",
    "settings.notifications.permission.default": "尚未授权",
    "settings.notifications.permission.granted": "已授权",
    "settings.notifications.permission.denied": "已拒绝",
    "settings.notifications.requestPermission": "请求通知权限",
    "settings.notifications.requesting": "正在请求…",
    "settings.notifications.categories": "通知类别",
    "settings.notifications.completed": "会话成功完成",
    "settings.notifications.failed": "会话执行失败",
    "settings.notifications.privacy": "通知隐私",
    "settings.notifications.privacy.description": "通知不会展示模型回答正文。会话标题可能出现在锁屏界面。",
    "settings.notifications.showTitle": "在通知中显示会话标题",
    "settings.notifications.limitations": "能力边界",
    "settings.notifications.browserLimitation": "浏览器页面关闭后无法继续接收本地服务事件；后台推送需要未来的 Service Worker 或远程推送端点。",
    "settings.notifications.desktopLimitation": "系统通知身份、图标和勿扰模式由已安装应用与操作系统控制。",
    "app.serviceConnectionRestored": "本地服务连接已恢复。",
    "app.serviceReconnecting": "本地服务连接已断开，正在重连...",
    "app.serviceCheckingRecovery": "本地服务连接已断开，正在检测恢复...",
    "area.chat": "工作台",
    "area.skills": "插件",
    "area.agentSkills": "技能",
    "area.agents": "智能体",
    "area.memory": "记忆",
    "area.knowledge": "知识库",
    "area.settings": "设置",
    "area.chat.description": "项目会话、消息流与模型输入区",
    "area.skills.description": "LuaSkills 技能、插件与运行时设置",
    "area.agentSkills.description": "标准 Agent Skills 技能目录",
    "area.agents.description": "智能体、专家画像与调度提示词",
    "area.memory.description": "VMM 用户、项目与会话记忆",
      "area.knowledge.description": "Markdown、Jieba 与 BM25 项目资料",
      "knowledge.name": "知识库名称",
      "knowledge.name.placeholder": "新知识库名称",
      "knowledge.create": "创建",
      "knowledge.filter.aria": "筛选知识库",
      "knowledge.filter.placeholder": "筛选知识库",
      "knowledge.refresh": "刷新",
      "knowledge.emptyLibraries": "尚无知识库。",
      "knowledge.libraryCounts": "{documents} 文档 · {chunks} 切片 · {status}",
      "knowledge.defaultDescription": "Markdown 原文、Jieba 分词与 BM25 检索。",
      "knowledge.saveName": "保存名称",
      "knowledge.view": "查看",
      "knowledge.importDirectory": "导入目录",
      "knowledge.sources.title": "导入目录",
      "knowledge.sources.description": "每个目录独立记录；同步时按源文件 SHA-256 仅更新新增、变化和删除的 Markdown 文档。",
      "knowledge.sources.add": "添加目录",
      "knowledge.sources.loading": "正在加载导入目录…",
      "knowledge.sources.empty": "尚未添加导入目录。",
      "knowledge.sources.documents": "{count} 份文档",
      "knowledge.sources.counts": "{documents} 份文档 · {chunks} 个切片",
      "knowledge.sources.lastScanned": "上次扫描 {time}",
      "knowledge.sources.sync": "增量同步",
      "knowledge.sources.modify": "修改目录",
      "knowledge.sources.modifyPickerTitle": "为“{name}”选择新的 Markdown 目录",
      "knowledge.cancelImport": "取消导入",
      "knowledge.cancelOperation": "取消操作",
      "knowledge.repairIndex": "补全索引",
      "knowledge.rebuildIndex": "重建索引",
      "knowledge.indexHealth.title": "索引完整性",
      "knowledge.indexHealth.description": "实时核对权威切片、物理 FTS 记录、内容版本和 Jieba 词典版本。",
      "knowledge.indexHealth.loading": "正在检查索引完整性…",
      "knowledge.indexHealth.searchable": "可检索",
      "knowledge.indexHealth.needsRepair": "需要补全",
      "knowledge.indexHealth.authoritative": "权威切片",
      "knowledge.indexHealth.indexed": "索引记录",
      "knowledge.indexHealth.missing": "缺失或错配",
      "knowledge.indexHealth.orphaned": "冗余、孤立或错配",
      "knowledge.indexHealth.contentRevision": "内容版本 {indexed} / {current}",
      "knowledge.indexHealth.dictionaryRevision": "词典版本 {indexed} / {current}",
      "knowledge.indexHealth.tokenizer": "分词器 {tokenizer}",
      "knowledge.delete": "删除",
      "knowledge.deleteConfirm": "确认永久删除知识库“{name}”？",
      "knowledge.projectAuthorization": "项目授权",
      "knowledge.documentProject": "文档浏览项目",
      "knowledge.sessionDefaultEnabled": "会话默认启用",
      "knowledge.currentSession": "当前会话启用",
      "knowledge.selectionExplicit": "显式选择",
      "knowledge.selectionInherited": "继承项目默认",
      "knowledge.documents": "文档",
      "knowledge.search.placeholder": "使用 Jieba BM25 搜索内容",
      "knowledge.search": "搜索",
      "knowledge.searching": "搜索中…",
      "knowledge.search.empty": "未找到匹配内容。",
      "knowledge.viewer.maximize": "最大化知识库阅读器",
      "knowledge.viewer.restore": "还原知识库阅读器",
      "knowledge.viewer.loadingDocuments": "正在加载资料目录…",
      "knowledge.viewer.loadingFolder": "正在加载…",
      "knowledge.viewer.loadMore": "加载更多",
      "knowledge.viewer.loadingDocument": "正在加载文档…",
      "knowledge.viewer.selectDocument": "请从左侧目录选择一份文档。",
      "knowledge.viewer.clearSearch": "清除搜索",
      "knowledge.viewer.searchCount": "{count} 份文档",
      "knowledge.search.substring": "子串匹配",
      "knowledge.body": "正文",
      "knowledge.documentCounts": "{lines} 行 · {chunks} 切片",
      "knowledge.emptyDocuments": "当前知识库暂无可浏览文档。",
      "knowledge.markdown.preview": "Markdown 预览",
      "knowledge.markdown.source": "查看原文",
      "knowledge.previousPage": "上一页",
      "knowledge.page": "第 {page} 页",
      "knowledge.nextPage": "下一页",
      "knowledge.close": "关闭",
      "knowledge.emptySelection": "创建或选择一个知识库开始使用。",
      "knowledge.importConfirm": "导入 Markdown",
      "knowledge.pickerTitle": "选择 Markdown 资料目录",
      "knowledge.nameRequired": "请输入知识库名称。",
      "knowledge.operationCompleted": "{operation}完成。",
      "knowledge.importProgress": "{phase} · {files} 文件 · {bytes} 字节",
      "knowledge.importCompleted": "导入完成：扫描 {scanned}，变化 {changed}，删除 {deleted}，切片 {chunks}。",
      "knowledge.importDialog.title": "导入 Markdown 资料",
      "knowledge.operationDialog.title.import": "导入 Markdown 资料",
      "knowledge.operationDialog.title.repair": "补全知识库索引",
      "knowledge.operationDialog.title.rebuild": "重建知识库索引",
      "knowledge.importDialog.preparing": "正在创建后台任务",
      "knowledge.importDialog.scanning": "正在扫描，已发现 {discovered} 个 Markdown 文件",
      "knowledge.importDialog.processing": "正在处理 {processed} / {total}（{percent}%）",
      "knowledge.importDialog.completed": "操作已完成",
      "knowledge.importDialog.cancelled": "操作已取消",
      "knowledge.importDialog.failed": "操作未完成",
      "knowledge.importDialog.discovered": "已发现 {count} 个 Markdown 文件",
      "knowledge.importDialog.total": "总计 {count} 个 Markdown 文件",
      "knowledge.importDialog.bytes": "已读取 {count} 字节",
      "knowledge.importDialog.indexItems": "索引进度 {processed} / {total}",
      "knowledge.importDialog.current": "当前目录、文件或阶段",
      "knowledge.importDialog.waiting": "等待任务进度…",
      "knowledge.importDialog.close": "关闭",
      "knowledge.importDialog.cancelling": "正在取消…",
      "knowledge.importDialog.cancellingDetail": "取消请求已提交，正在停止当前批次…",
      "knowledge.importDialog.committing": "正在原子提交资料内容…",
      "knowledge.importDialog.indexing": "正在更新搜索索引 {processed} / {total}（{percent}%）",
      "knowledge.importDialog.repairing": "正在补全搜索索引 {processed} / {total}（{percent}%）",
      "knowledge.importDialog.rebuilding": "正在恢复搜索索引 {processed} / {total}（{percent}%）",
      "knowledge.importDialog.verifying": "正在校验搜索索引…",
      "knowledge.importDialog.finalizing": "正在完成导入并清理暂存数据…",
      "knowledge.importDialog.committingFallback": "正在执行知识库原子提交",
      "knowledge.importDialog.indexingFallback": "正在更新增量搜索索引",
      "knowledge.importDialog.repairingFallback": "正在补全缺失或错配的搜索索引",
      "knowledge.importDialog.rebuildingFallback": "正在从权威切片恢复搜索索引",
      "knowledge.importDialog.verifyingFallback": "正在检查索引版本与分词器",
      "knowledge.importDialog.finalizingFallback": "正在更新知识库统计并清理暂存数据",
      "knowledge.importPhase.scanning": "扫描文件",
      "knowledge.importPhase.parsing": "解析文件",
      "knowledge.importPhase.committing": "提交内容",
      "knowledge.importPhase.indexing": "更新索引",
      "knowledge.importPhase.repairing": "补全索引",
      "knowledge.importPhase.rebuilding": "恢复索引",
      "knowledge.importPhase.verifying": "校验索引",
      "knowledge.importPhase.finalizing": "完成导入",
      "knowledge.importPhase.completed": "已完成",
      "knowledge.importPhase.cancelled": "已取消",
      "knowledge.composer.aria": "选择本会话知识库",
      "knowledge.composer.label": "知识库",
      "knowledge.composer.manage": "知识库管理",
      "knowledge.composer.noneAuthorized": "当前项目没有已授权知识库",
      "knowledge.package.import": "导入完整库",
      "knowledge.package.export": "导出完整库",
      "knowledge.package.exportPickerTitle": "选择知识库包导出目录",
      "knowledge.package.importPickerTitle": "选择 .vkb 知识库包",
      "knowledge.package.noPackages": "当前目录没有子目录或 .vkb 知识库包。",
      "knowledge.package.inspecting": "正在检查…",
      "knowledge.package.inspect": "检查并继续",
      "knowledge.package.importTitle": "导入完整知识库",
      "knowledge.package.counts": "{documents} 文档 · {chunks} 切片 · {bytes}",
      "knowledge.package.nameConflict": "知识库名称“{name}”已存在。默认不会覆盖，请改名导入或显式选择覆盖。",
      "knowledge.package.createStrategy": "新建知识库",
      "knowledge.package.renameRequired": "请修改为一个未使用的名称；原知识库不会被改动。",
      "knowledge.package.createDescription": "使用包内原名称创建新知识库，并保留现有知识库。",
      "knowledge.package.replaceStrategy": "覆盖现有知识库",
      "knowledge.package.replaceDescription": "保留目标知识库标识、项目授权和会话引用，用包内完整数据替换内容。",
      "knowledge.package.replaceConfirm": "确认覆盖知识库“{name}”？当前 {documents} 份文档、{chunks} 个切片将被包内数据替换。该操作会保留知识库标识、项目授权与会话引用。",
      "knowledge.package.exportCompleted": "知识库包已导出到：{path}",
      "knowledge.package.importCompleted": "完整知识库已导入，无需重建索引。",
      "knowledge.package.replaceCompleted": "知识库已安全覆盖，原授权与会话引用已保留。",
    "area.settings.description": "项目、模型、工具、诊断与系统设置",
    "feature.status.available": "可用",
    "feature.status.preview": "预览",
    "global.openSettings": "打开设置",
    "global.toggleDark": "切换为暗色主题",
    "global.toggleLight": "切换为亮色主题",
    "global.language": "语言",
    "global.languageSaveFailed": "保存语言失败：{error}",
    "global.online": "在线",
    "global.preview": "预览",
    "global.progress": "进度",
    "global.labelSeparator": "：",
    "global.listDelimiter": "、",
    "appChrome.menu": "打开应用菜单",
    "appChrome.status": "状态",
    "appChrome.theme": "主题",
    "appChrome.theme.dark": "暗色",
    "appChrome.theme.light": "亮色",
    "appChrome.language.zh": "中文",
    "appChrome.scrollFeaturesLeft": "向左滚动主功能区",
    "appChrome.scrollFeaturesRight": "向右滚动主功能区",
    "appChrome.windowActionFailed": "窗口操作失败：",
    "appChrome.window.minimize": "最小化窗口",
    "appChrome.window.restore": "还原窗口",
    "appChrome.window.maximize": "最大化窗口",
    "appChrome.window.close": "关闭窗口",
    "appChrome.githubOpenFailed": "打开 GitHub 失败：",
    "appChrome.githubAria": "打开 OpenVulcan/vulcan-code GitHub 仓库",
    "sidebar.user.local": "本地用户",
    "sidebar.user.account": "账户",
    "sidebar.user.logout": "退出登录",
    "projectMenu.conversationsAria": "{project} 会话",
    "projectMenu.newConversation": "新建会话",
    "projectMenu.noConversations": "暂无会话",
    "projectMenu.pin": "置顶项目",
    "projectMenu.unpin": "取消置顶项目",
    "projectMenu.openDirectory": "在资源管理器中打开",
    "projectMenu.createWorktree": "创建永久工作树",
    "projectMenu.renameProject": "重命名项目",
    "projectMenu.setIcon": "设置项目图标",
    "projectMenu.archiveConversations": "归档对话",
    "projectMenu.removeProject": "移除项目",
    "projectMenu.projectSettings": "项目设置",
    "conversationMenu.pin": "置顶对话",
    "conversationMenu.unpin": "取消置顶对话",
    "conversationMenu.rename": "重命名对话",
    "conversationMenu.archive": "归档对话",
    "conversationMenu.markUnread": "标记为未读",
    "conversationMenu.openDirectory": "在资源管理器中打开",
    "conversationMenu.copyTitle": "复制标题",
    "conversationMenu.copyWorkspace": "复制工作目录",
    "conversationMenu.copySessionId": "复制会话 ID",
    "conversationMenu.copyDeepLink": "复制深度链接",
    "conversationMenu.forkLocal": "派生到本地",
    "conversationMenu.forkWorktree": "派生到新工作树",
    "conversationMenu.openNewWindow": "在新窗口中打开",
    "conversationMenu.moreTitleActions": "更多标题操作",
    "diffAction.moreStaging": "更多暂存操作",
    "diffAction.unstageAll": "取消全部暂存",
    "diffAction.unstageFile": "取消当前文件暂存",
    "diffAction.restoreAll": "还原全部",
    "diffAction.stageAll": "暂存全部",
    "diffAction.stageFile": "暂存此文件",
    "diffAction.restoreFile": "还原此文件",
    "diff.mode.unstaged": "未暂存",
    "diff.mode.staged": "已暂存",
    "diff.mode.commit": "未推送提交",
    "diff.mode.branch": "分支",
    "diff.branch.current": "当前分支",
    "diff.branch.main": "主分支",
    "diff.view.toUnified": "切换到统一视图",
    "diff.view.toSplit": "切换到分栏视图",
    "diff.refresh": "刷新 diff",
    "diff.loading": "加载中",
    "diff.moreActions": "更多 diff 操作",
    "diff.copyRaw": "复制 raw diff",
    "diff.inline.disable": "禁用文字差异",
    "diff.inline.enable": "启用文字差异",
    "diff.wrap.disable": "禁用自动换行",
    "diff.wrap.enable": "启用自动换行",
    "diff.contextControls.hide": "隐藏上下文展开控件",
    "diff.contextControls.show": "显示上下文展开控件",
    "diff.indicator.useClassic": "使用 +/- 行标",
    "diff.indicator.useBars": "使用颜色线条行标",
    "diff.search.open": "打开 diff 搜索",
    "diff.search.label": "搜索 diff",
    "diff.search.truncated": "结果已截断",
    "diff.search.previous": "上一条",
    "diff.search.next": "下一条",
    "diff.search.indexUnavailable": "索引不可用",
    "diff.commit.none": "暂无未推送提交",
    "diff.branch.search": "搜索分支",
    "diff.branch.label": "分支",
    "diff.branch.noMatches": "没有匹配分支",
    "diff.emptyBody": "当前没有可展示的 Git Diff。",
    "diff.noMatchingFiles": "没有匹配的文件",
    "diff.file.tooLarge": "超大文件未展示",
    "diff.file.added": "新增文件",
    "diff.file.deleted": "删除文件",
    "diff.file.gitlink": "子模块指针变更，未渲染文本内容。",
    "diff.file.renamedNoChanges": "File renamed without changes",
    "diff.file.noContent": "No content",
    "diff.render.changedLinesSkipped": "变更行超过 {limit}，高亮细节已按轻量策略省略",
    "diff.render.skipped": "高亮细节已按轻量策略省略",
    "diff.render.rowsLightweight": "{count} 行按轻量模式渲染",
    "diff.large.totalFiles": "文件数超过 {limit}",
    "diff.large.totalChangedLines": "总变更行超过 {limit}",
    "diff.large.totalChangedBytes": "总变更体积超过 {limit}",
    "diff.large.fileChangedLines": "单文件变更行超过 {limit}",
    "diff.large.fileChangedBytes": "单文件变更体积超过 {limit}",
    "diff.large.fileMaxLineBytes": "单行体积超过 {limit}",
    "diff.load.rowsPending": "{rows} 行待加载，约 {hunks} 个 hunk",
    "diff.load.retry": "重试",
    "diff.load.reveal": "打开位置",
    "diff.load.matchCount": "{count} 条命中",
    "diff.load.backendMatchFallback": "后端完整 diff 命中该文件。",
    "diff.load.loadingDetail": "正在加载文件 diff 详情。",
    "diff.load.timeout": "文件 diff 详情加载超时。",
    "diff.load.failed": "文件 diff 详情加载失败。",
    "diff.load.loadedEmpty": "文件 diff 详情已加载，但没有可展示的文本内容。",
    "diff.load.summaryIdle": "该文件为摘要状态，进入视口后将按需加载详情。",
    "console.title": "会话",
    "console.newConversation": "新会话",
    "console.emptyPrompt": "我们应该在 {projectName} 中构建什么？",
    "conversation.loading.title": "正在载入会话",
    "conversation.loading.requesting": "正在请求会话数据",
    "conversation.loading.locating": "正在定位会话",
    "conversation.loading.index": "正在读取消息索引",
    "conversation.loading.reading": "正在读取消息内容",
    "conversation.loading.subagents": "正在恢复未关闭智能体索引",
    "conversation.loading.projecting": "正在整理显示数据",
    "conversation.loading.done": "会话载入完成",
    "conversation.loading.failed": "会话正文载入失败",
    "conversation.unlinkedProject": "未关联项目",
    "conversation.history.loadMore": "加载更多历史",
    "conversation.history.loading": "正在加载历史",
    "conversation.action.copyFinal": "复制最终输出原文",
    "conversation.action.copiedFinal": "已复制最终输出原文",
    "conversation.action.fork": "分叉会话",
    "conversation.action.forkTitleSuffix": "（分叉）",
    "conversation.action.forkFailed": "分叉失败",
    "conversation.action.forkSucceeded": "已创建分叉会话",
    "conversation.action.forkInProgressTitle": "正在分叉会话",
    "conversation.action.forkInProgressMessage": "正在复制会话并打开新分叉，请稍候。",
    "conversation.forkSource.label": "从对话中派生",
    "conversation.forkSource.missing": "原会话已经不存在",
    "conversation.block.toolCall": "调用工具",
    "conversation.block.toolResult": "工具返回",
    "conversation.block.command": "命令会话",
    "conversation.block.thinking": "思考过程",
    "conversation.block.plan": "计划",
    "conversation.block.notice": "运行提示",
    "conversation.block.detail": "详情",
    "conversation.metadata.type": "类型",
    "conversation.metadata.status": "状态",
    "conversation.metadata.tool": "工具",
    "conversation.metadata.identifier": "标识",
    "conversation.metadata.error": "错误",
    "conversation.metadata.result": "结果",
    "conversation.metadata.returned": "已返回",
    "conversation.toolRequest.receiving": "正在接收数据",
    "conversation.toolRequest.ready": "数据接收完成",
    "conversation.toolRequest.characters": "个字符",
    "conversation.stats.providerCalls": "{count} 次调用",
    "conversation.stats.toolCalls": "{count} 个工具",
    "conversation.stats.totalTokens": "{count} tokens",
    "conversation.stats.cachedTokens": "缓存命中 {count}",
    "conversation.stats.cachedTokensCompact": "缓存 {count} tokens",
    "conversation.stats.outputSpeed": "{speed}/s",
    "conversation.stats.group.regular": "常规 {count} tokens",
    "conversation.stats.group.subagent": "智能体 {count} tokens",
    "conversation.stats.group.contextCompaction": "压缩 {count} tokens",
    "conversation.subagent.stopTask": "停止",
    "conversation.stopWithAgents.title": "停止当前会话？",
    "conversation.stopWithAgents.description": "当前还有 {count} 个智能体正在工作。请选择只停止主会话，还是同时停止这些智能体。",
    "conversation.stopWithAgents.mainOnly": "只停止主会话",
    "conversation.stopWithAgents.all": "同时停止智能体",
    "conversation.process.header": "处理过程",
    "conversation.process.stoppedAfter": "你在 {duration} 后停止了",
    "conversation.process.stopped": "你已停止本轮工作",
    "conversation.process.duration": "持续 {duration}",
    "conversation.toolSequence.summary": "已执行 {count} 次工具调用",
    "conversation.toolSequence.runningSummary": "已执行 {completed} 次调用，正在调用 {running} 个工具",
    "conversation.action.editPrompt": "编辑用户输入",
    "conversation.action.editPromptFailed": "编辑用户输入失败",
    "conversation.action.cancel": "取消",
    "conversation.action.saveAndReplay": "保存并重新执行",
    "contextCompaction.title.running": "上下文正在压缩",
    "contextCompaction.title.completed": "上下文已压缩",
    "contextCompaction.title.failed": "上下文压缩失败",
    "contextCompaction.generating": "正在生成上下文压缩总结。",
    "contextCompaction.thinkingLabel": "思考过程",
    "contextCompaction.thinkingActive": "正在思考",
    "contextCompaction.emptyThinking": "暂无思考内容",
    "contextCompaction.failed.trigger": "触发方式",
    "contextCompaction.failed.error": "错误",
    "contextCompaction.failed.note": "API 压缩未完成，未改写上下文锚点；后续请求仍沿用原上下文窗口。",
    "contextCompaction.trigger.manual": "手动",
    "contextCompaction.trigger.auto": "自动",
    "contextCompaction.progress.prepareInput": "正在准备压缩输入。",
    "contextCompaction.progress.buildRequest": "已准备压缩输入，正在构建模型总结请求。",
    "contextCompaction.progress.prepareRuntime": "正在准备独立压缩运行时与工具快照。",
    "contextCompaction.progress.runtimeReady": "压缩运行时已准备完成，正在启动 Provider 请求。",
    "contextCompaction.progress.prepared": "模型请求准备完成。",
    "contextCompaction.progress.requestReady": "模型请求快照已构建，正在转换为 Provider 载荷。",
    "contextCompaction.progress.providerCheckpoint": "已记录 Provider 请求检查点。",
    "contextCompaction.progress.providerLoweringRepair": "Provider 请求转换已执行兼容性修复。",
    "contextCompaction.progress.wireRequestReady": "Provider 请求载荷已构建，准备发送。",
    "contextCompaction.progress.requestStarted": "Provider 请求已发送，正在等待首个输出。",
    "contextCompaction.progress.thinkingDelta": "模型已开始输出思考内容。",
    "contextCompaction.progress.outputDelta": "模型已开始生成压缩总结。",
    "contextCompaction.progress.prepareExecution": "正在准备压缩执行环境与工具快照。",
    "contextCompaction.progress.executionReady": "压缩执行环境已准备完成，正在启动运行时。",
    "contextCompaction.progress.bootstrapRuntime": "正在启动独立压缩运行时。",
    "contextCompaction.progress.unknown": "正在执行上下文压缩。",
    "contextPanel.open": "打开右侧面板",
    "contextPanel.close": "关闭右侧面板",
    "contextPanel.review": "审核",
    "contextPanel.memory": "记忆",
    "contextPanel.files": "文件",
    "contextPanel.dynamic": "动态",
    "contextPanel.static": "静态",
    "contextPanel.reviewEmpty": "Git Diff 审核面板待接入",
    "contextPanel.memoryEmpty": "记忆注入信息面板待接入",
    "contextPanel.dynamicEmpty": "当前会话没有有效的动态 TTL 注入。",
    "contextPanel.staticEmpty": "当前会话没有有效的静态上下文。",
    "contextUsage.title": "上下文占用",
    "contextUsage.waiting": "请求拆分：等待预算引擎计算",
    "sidebar.console": "工作台",
    "sidebar.pinned": "置顶",
    "sidebar.projects": "项目",
    "sidebar.unlinkedConversations": "未关联项目",
    "sidebar.addProject": "添加项目",
    "sidebar.noProjects": "暂无项目",
    "sidebar.waitingWorkspace": "等待 Workspace 快照载入",
    "sidebar.overview": "概览",
    "sidebar.settingsCategories": "设置分类",
    "sidebar.toggle": "切换侧栏",
    "composer.mode.execute": "构建",
    "composer.mode.plan": "计划",
    "composer.capability.extended": "扩展能力",
    "composer.capability.extended.subtitle": "能力扩展设置",
    "composer.capability.basic": "基础能力",
    "composer.capability.basic.subtitle": "仅文本与代码",
    "composer.capability.enabledCount": "{enabled}/{total} 已启用",
    "composer.capability.tooltipTitle": "点击设置扩展能力",
    "composer.capability.tooltipEmpty": "暂无已启用模块",
    "composer.capability.dialogTitle": "当前会话能力设置",
    "composer.capability.dialogDescription": "调整排序与勾选后需要点击保存，保存后会写入当前会话的独立配置，并影响下一次对话的缓存命中。",
    "composer.capability.source.nativeWithTargets": "模型原生 + {count} 个目标",
    "composer.capability.source.native": "模型原生",
    "composer.capability.source.targets": "{count} 个目标",
    "composer.capability.source.noTargets": "暂无可用目标",
    "composer.capability.empty.native": "当前能力由所选模型原生处理。",
    "composer.capability.empty.noTargets": "当前能力暂无已配置目标。",
    "composer.capability.saveHint.externalUpdate": "其他窗口已更新此会话配置，当前草稿尚未同步。",
    "composer.capability.saveHint.saving": "正在保存到当前会话...",
    "composer.capability.saveHint.unsaved": "有未保存变更。",
    "composer.capability.saveHint.clean": "当前没有未保存变更。",
    "composer.capability.useDefaults": "使用默认配置",
    "composer.capability.save.saving": "保存中",
    "composer.capability.save.saved": "已保存",
    "composer.capability.save": "保存",
    "composer.capability.close.title": "保存更改？",
    "composer.capability.close.description": "当前会话能力设置有未保存变更。",
    "composer.capability.close.discard": "不保存",
    "composer.capability.close.keepEditing": "继续编辑",
    "composer.reasoning.off": "关闭",
    "composer.reasoning.low": "低",
    "composer.reasoning.medium": "中",
    "composer.reasoning.high": "高",
    "composer.reasoning.max": "Max",
    "composer.reasoning.subtitle": "推理等级",
    "composer.placeholder": "输入当前会话要执行的任务指令（{shortcut}）",
    "composer.shortcutHint": "⏎ 发送，Shift + ⏎ 换行",
    "composer.selectFallback": "选择",
    "composer.aria.input": "当前会话输入",
    "composer.aria.mode": "选择构建模式",
    "composer.aria.model": "选择模型",
    "composer.aria.capability": "选择扩展能力",
    "composer.aria.reasoning": "选择推理等级",
    "composer.aria.send": "发送",
    "composer.aria.stop": "停止",
    "composer.modelPicker.filter": "输入模型名称、ID 或 Provider 过滤",
    "composer.modelPicker.noMatches": "当前过滤条件下没有匹配模型。",
    "composer.attachment.add": "添加附件",
    "composer.attachment.extension": "扩展",
    "composer.attachment.menu": "附件",
    "composer.attachment.empty": "当前无可用附件类型",
    "composer.attachment.remove": "移除附件",
    "composer.attachment.unsupported": "当前模型或扩展能力不支持这些附件。",
    "composer.attachment.maxLimit": "最多只能添加 {count} 个附件。",
    "composer.attachment.partialMaxLimit": "已达到 {count} 个附件上限，部分文件未添加。",
    "composer.attachment.kind.image": "图片",
    "composer.attachment.kind.audio": "音频",
    "composer.attachment.kind.video": "视频",
    "composer.attachment.kind.pdf": "PDF",
    "composer.thinkingProcess": "思考过程",
    "composer.planMode": "计划模式",
    "composer.vmmMemory": "VMM 记忆",
    "composer.vmm.precheckInjection": "前置记忆注入",
    "composer.vmm.memoryWriteback": "后置记忆提交",
    "composer.vmm.mode.inheritProject": "跟随项目",
    "composer.vmm.mode.enabled": "开启",
    "composer.vmm.mode.disabled": "关闭",
    "composer.goal": "目标",
    "composer.scrollToBottom": "滚动到底部",
    "composer.goalDialog.resumeTitle": "继续任务",
    "composer.goalDialog.editTitle": "编辑目标",
    "composer.goalDialog.objectiveAria": "目标内容",
    "composer.goalDialog.objectivePlaceholder": "输入当前会话要持续推进的目标",
    "composer.goalDialog.tokenBudgetAria": "Token 预算",
    "composer.goalDialog.tokenBudgetPlaceholder": "Token 预算（可选）",
    "composer.goalDialog.save": "保存",
    "composer.goal.status.active": "进行中的目标",
    "composer.goal.status.paused": "已暂停的目标",
    "composer.goal.status.blocked": "受阻的目标",
    "composer.goal.status.usageLimited": "受限的目标",
    "composer.goal.status.budgetLimited": "预算用尽",
    "composer.goal.status.completed": "已完成的目标",
    "composer.goal.tooltip.input": "输入（新增）",
    "composer.goal.tooltip.cache": "输入（缓存）",
    "composer.goal.tooltip.output": "输出",
    "composer.goal.tooltip.reasoning": "思考",
    "composer.goal.tooltip.total": "总量",
    "composer.goal.action.edit": "编辑目标",
    "composer.goal.action.resumeTask": "继续任务",
    "composer.goal.action.resume": "恢复目标",
    "composer.goal.action.pause": "暂停目标",
    "composer.goal.action.clear": "清理目标",
    "composer.goal.error.empty": "目标内容不能为空。",
    "composer.goal.error.budgetInvalid": "Token 预算必须是正整数。",
    "composer.planMode.label": "计划",
    "composer.planMode.goalConflict": "计划模式与目标模式不能同时开启。",
    "composer.error.stopCurrentFailed": "停止当前会话失败。",
    "composer.error.promptEmpty": "请输入任务指令。",
    "composer.modelPicker.tooltipTitle": "点击选择模型",
    "composer.modelPicker.currentModel": "当前模型：{model}",
    "composer.modelPicker.currentProvider": "当前供应商：{provider}",
    "composer.secondaryRoute.select": "选择二级供应商",
    "composer.secondaryRoute.search": "搜索二级供应商",
    "composer.secondaryRoute.autoEndpoint": "由 OpenRouter 自动选择 endpoint",
    "composer.secondaryRoute.loading": "正在加载二级供应商...",
    "composer.secondaryRoute.noMatches": "没有匹配的二级供应商",
    "composer.secondaryRoute.switchFailed": "二级供应商切换失败，已保留原选择。",
    "composer.secondaryRoute.switched": "二级供应商已切换。",
    "composer.secondaryRoute.restoredAuto": "二级供应商已恢复 Auto。",
    "composer.progress.open": "查看当前进程",
    "composer.progress.progressLabel": "进程进度 {current}/{total}",
    "composer.progress.step": "第 {current} / {total} 步",
    "composer.progress.changedFiles": "{count} 个文件已更改",
    "composer.progress.stepInProgress": "步骤进行中",
    "composer.reasoning.tooltip": "思考级别",
    "pendingPrompt.queue": "待发送队列",
    "pendingPrompt.drag": "拖动待发送提示词",
    "pendingPrompt.delete": "删除待发送提示词",
    "pendingPrompt.edit": "编辑待发送提示词",
    "pendingPrompt.insert": "引导待发送提示词",
    "pendingPrompt.inserted": "已引导待发送提示词",
    "pendingPrompt.insertShort": "引导",
    "pendingPrompt.insertedShort": "已引导",
    "pendingPrompt.attachmentCount": "{count} 个附件",
    "pendingPrompt.attachmentMessage": "附件消息",
    "sessionActions.noProjectForCapability": "当前会话没有明确关联项目，无法保存 AI 能力设置。",
    "sessionActions.noConversationForCapability": "请先创建或选择一个会话，再保存 AI 能力设置。",
    "sessionActions.previewCannotSaveCapability": "浏览器预览模式下无法保存会话 AI 能力设置。",
    "sessionActions.capabilitySaved": "已保存当前会话 AI 能力设置。",
    "sessionActions.noConversationForGoalSave": "请先创建或选择一个会话，再设置目标。",
    "sessionActions.previewCannotSaveGoal": "浏览器预览模式下无法保存目标。",
    "sessionActions.goalSaved": "目标已保存。",
    "sessionActions.noExistingConversation": "请先选择一个已有会话。",
    "sessionActions.previewCannotResumeGoal": "浏览器预览模式下无法继续目标。",
    "sessionActions.goalResumed": "目标已继续。",
    "sessionActions.previewCannotClearGoal": "浏览器预览模式下无法清理目标。",
    "sessionActions.goalCleared": "目标已清理。",
    "sessionActions.previewCannotUpdateGoal": "浏览器预览模式下无法更新目标状态。",
    "sessionActions.noConversationForPlanMode": "请先创建或选择一个会话，再切换计划模式。",
    "sessionActions.previewCannotTogglePlanMode": "浏览器预览模式下无法切换计划模式。",
    "sessionActions.planModeEnabled": "计划模式已开启。",
    "sessionActions.planModeDisabled": "计划模式已关闭。",
    "sessionActions.noConversationForVmmPolicy": "请先创建或选择一个会话，再切换 VMM 记忆策略。",
    "sessionActions.previewCannotSetVmmPolicy": "浏览器预览模式下无法切换 VMM 记忆策略。",
    "sessionActions.vmmPolicySaved": "VMM 记忆策略已保存。",
    "conversationActions.noProjectForSend": "当前会话没有明确关联项目，无法发送。",
    "conversationActions.previewCannotSend": "浏览器预览模式下无法发送真实会话。",
    "conversationActions.previewCannotCompact": "浏览器预览模式下无法执行真实上下文压缩。",
    "conversationActions.modelSwitched": "当前模型已切换为 {model}（{provider}）。",
    "projectActions.vmmSaved": "已保存项目 VMM 设置。",
    "projectActions.openRequested": "已请求打开项目目录：{project}",
    "projectActions.unpinned": "已取消置顶项目：{project}",
    "projectActions.pinned": "已置顶项目：{project}",
    "projectActions.iconUpdated": "已更新项目图标：{project}",
    "projectActions.iconReset": "已还原项目图标：{project}",
    "projectActions.listUpdated": "项目列表已更新。",
    "projectActions.skillInherited": "已将 {skill} 改为继承全局设置",
    "projectActions.skillEnabled": "已在项目中启用 {skill}",
    "projectActions.skillDisabled": "已在项目中关闭 {skill}",
    "projectSettings.cleanupCompleted": "项目附件缓存清理已完成。",
    "projectDialog.renameProjectTitle": "重命名项目",
    "projectDialog.renameConversationTitle": "重命名对话",
    "projectDialog.renameProjectDescription": "输入新的项目显示名称。项目路径不会改变；留空确认会还原为项目文件夹名。",
    "projectDialog.renameConversationDescription": "输入新的对话名称。",
    "projectDialog.renamePlaceholder": "输入新的名称",
    "projectDialog.confirm": "确认",
    "projectDialog.archiveRemoveTitle": "归档并移除项目？",
    "projectDialog.archiveTitle": "归档 {count} 个对话？",
    "projectDialog.archiveRemoveDescription": "这会先归档 {project} 中的 {count} 个对话，然后从项目列表移除该项目。重新添加这个项目后，已归档对话才会出现在归档恢复中。",
    "projectDialog.archiveDescription": "这会将 {project} 中的对话归档，项目本身会保留在左侧项目列表中。",
    "projectDialog.currentProject": "当前项目",
    "projectDialog.archiveAndRemove": "归档并移除",
    "projectDialog.archiveAll": "全部归档",
    "projectDialog.removing": "移除中",
    "projectDialog.archiving": "归档中",
    "projectRow.conversationMenuAria": "{project} 会话菜单",
    "projectRow.createConversationAria": "在 {project} 中创建新会话",
    "projectRow.collapseConversations": "折叠显示",
    "projectRow.showMoreConversations": "展开显示",
    "projectRow.noConversations": "暂无对话",
    "conversationRow.running": "会话正在运行",
    "conversationRow.unread": "未读",
    "conversationRow.askAttention": "有待回答问题",
    "conversationRow.planAttention": "有计划确认",
    "conversationRow.confirmArchive": "确认归档对话",
    "projectFolderPicker.title": "选择项目目录",
      "projectFolderPicker.pathAria": "目录路径",
      "projectFolderPicker.openPath": "打开输入的路径",
    "projectFolderPicker.parent": "上级",
    "projectFolderPicker.rootsAria": "盘符",
    "projectFolderPicker.noRoots": "没有可用盘符",
    "projectFolderPicker.hidden": "隐藏",
    "projectFolderPicker.noChildren": "没有可进入的子目录",
    "projectFolderPicker.adding": "添加中",
    "projectIcon.title": "设置项目图标",
    "projectIcon.description": "为 {project} 选择固定图标，或生成一个字母徽标。还原后会重新使用默认文件夹开关图标。",
    "projectIcon.previewAria": "项目图标预览",
    "projectIcon.customTab": "自定义",
    "projectIcon.filterPlaceholder": "过滤 {pack} 图标",
    "projectIcon.loadMore": "加载更多",
    "projectIcon.text": "文字",
    "projectIcon.textPlaceholder": "中文 1 个或英文 2 个",
    "projectIcon.shape": "形状",
    "projectIcon.shapeCircle": "圆形",
    "projectIcon.shapeSquare": "方形",
    "projectIcon.backgroundColor": "背景色",
    "projectIcon.borderColor": "边框色",
    "projectIcon.foregroundColor": "字体色",
    "projectIcon.resetDefault": "还原默认",
    "projectIcon.set": "设置",
    "projectSettings.dialogTitle": "项目设置：{project}",
    "projectSettings.dialogDescription": "项目级配置与缓存清理只作用于当前项目空间。",
    "projectSettings.categoryAria": "项目设置分类",
    "projectSettings.overview": "概览",
    "projectSettings.pinnedProject": "已置顶项目",
    "projectSettings.projectSpaceSummary": "项目空间摘要",
    "projectSettings.vmmMemory": "VMM 记忆",
    "projectSettings.knowledgeBases": "知识库",
    "projectSettings.knowledgeBasesDetail": "项目授权与会话继承默认值",
    "projectSettings.knowledgeBasesDescription": "授权决定本项目可使用的知识库；默认启用决定新会话继承项目设置时自动启用哪些库。",
    "projectSettings.knowledgeBasesLoading": "正在加载知识库授权…",
    "projectSettings.knowledgeBasesSaved": "知识库授权已保存。",
    "projectSettings.projectOverride": "项目覆盖",
    "projectSettings.inheritGlobal": "继承全局",
    "projectSettings.promptSkillCount": "{count} 个技能",
    "projectSettings.attachmentCache": "附件缓存",
    "projectSettings.attachmentCleanup": "历史附件清理",
    "projectSettings.projectSpace": "项目空间",
    "projectSettings.projectId": "项目标识：{id}",
    "projectSettings.conversationCount": "对话数量：{count}",
    "projectSettings.pinStatus": "置顶状态：{status}",
    "projectSettings.pinned": "已置顶",
    "projectSettings.notPinned": "未置顶",
    "projectSettings.vmmHasOverride": "当前项目已启用独立覆盖。",
    "projectSettings.vmmInheritsGlobal": "当前项目继承全局 VMM 设置。",
    "projectSettings.agentOverrideCount": "当前项目有 {count} 条覆盖。",
    "projectSettings.agentInheritsGlobal": "未设置项目级覆盖，继承全局规则。",
    "projectSettings.vmmTitle": "项目级 VMM 记忆",
    "projectSettings.codebaseMemory.title": "代码库记忆",
    "projectSettings.codebaseMemory.description": "为当前项目启用宿主管理的多项目代码图谱、检索与变更感知能力。",
    "projectSettings.codebaseMemory.enabled": "已启用托管服务",
    "projectSettings.codebaseMemory.disabled": "默认关闭",
    "projectSettings.codebaseMemory.loading": "正在加载 Codebase Memory 状态...",
    "projectSettings.codebaseMemory.enable": "为此项目启用 Codebase Memory",
    "projectSettings.codebaseMemory.enableHint": "项目会加入共享托管服务；会话只获得本项目的只读工具。",
    "projectSettings.codebaseMemory.disableHint": "不会注册 AI 工具，也不会删除已有索引。",
    "projectSettings.codebaseMemory.serviceStatus": "托管服务状态",
    "projectSettings.codebaseMemory.indexStatus": "项目索引状态",
    "projectSettings.codebaseMemory.lastIndexed": "最近成功索引",
    "projectSettings.codebaseMemory.revision": "索引修订",
    "projectSettings.codebaseMemory.unavailable": "暂不可用",
    "projectSettings.codebaseMemory.reindex": "重建项目索引",
    "projectSettings.codebaseMemory.enabledSaved": "已启用项目 Codebase Memory。",
    "projectSettings.codebaseMemory.disabledSaved": "已停用项目 Codebase Memory，已有索引仍保留。",
    "projectSettings.codebaseMemory.reindexAccepted": "项目索引重建请求已提交。",
    "projectSettings.vmmDescription": "覆盖当前项目的记忆开关、用户/项目映射与召回注入参数。",
    "projectSettings.vmmLoading": "正在加载项目 VMM 设置...",
    "projectSettings.enableProjectOverride": "启用项目覆盖",
    "projectSettings.overrideEnabledHint": "保存后当前项目使用下方设置。",
    "projectSettings.overrideDisabledHint": "保存后清空项目覆盖并继承全局设置。",
    "projectSettings.vmmTransportHint": "项目级设置继承全局 gRPC 连接与 API Key；连接参数请在 VMM 记忆的全局配置中调整。",
    "projectSettings.saveVmm": "保存 VMM 设置",
    "projectSettings.agentTitle": "项目级 Agent Skills",
    "projectSettings.agentDescription": "对系统与用户技能建立当前项目专属启停覆盖。",
    "projectSettings.agent.globalEnabled": "全局已启用",
    "projectSettings.agent.globalDisabled": "全局已关闭",
    "projectSettings.agentLoading": "正在加载项目技能覆盖...",
    "projectSettings.agentEmpty": "系统与用户目录暂未发现技能。",
    "projectSettings.agent.projectEnabled": "项目启用",
    "projectSettings.agent.projectDisabled": "项目关闭",
    "projectSettings.agent.inheritEnabled": "继承启用",
    "projectSettings.agent.inheritDisabled": "继承关闭",
    "projectSettings.agent.optionInheritEnabled": "继承全局（启用）",
    "projectSettings.agent.optionInheritDisabled": "继承全局（关闭）",
    "projectSettings.agent.projectStateAria": "{skill} 项目状态",
    "projectSettings.cleanupTitle": "项目附件缓存清理",
    "projectSettings.cleanupDescription": "按日期和附件类型筛选当前项目的托管历史附件。",
    "projectSettings.managedDirectory": "托管目录",
    "projectSettings.cutoffDate": "截止日期",
    "projectSettings.attachmentKind.image": "图片",
    "projectSettings.attachmentKind.audio": "音频",
    "projectSettings.attachmentKind.video": "视频",
    "projectSettings.cleanup.previewing": "预览中",
    "projectSettings.cleanup.preview": "预览清理范围",
    "projectSettings.cleanup.cleaning": "清理中",
    "projectSettings.cleanup.execute": "执行清理",
    "projectSettings.lastPreview": "最近一次预览",
    "projectSettings.lastExecution": "最近一次执行",
    "projectSettings.cleanup.notExecuted": "尚未执行历史附件缓存清理。",
    "projectSettings.cleanup.previewFallback": "预览模式将检查 {date} 之前的 {kinds} 托管附件。",
    "projectSettings.cleanup.invalidDate": "请选择有效的截止日期。",
    "projectSettings.cleanup.selectKind": "至少选择一种附件类型后才能预览。",
    "projectSettings.cleanup.initialNotice": "选择截止日期和附件类型后可预览当前项目的托管附件清理范围。",
    "shell.windowOperationFailed": "窗口操作失败：{error}",
    "shell.openGithubFailed": "打开 GitHub 失败：{error}",
    "shell.minimizeWindow": "最小化窗口",
    "shell.maximizeWindow": "最大化窗口",
    "shell.restoreWindow": "还原窗口",
    "shell.closeWindow": "关闭窗口",
    "shell.openRepository": "打开 OpenVulcan/vulcan-code GitHub 仓库",
    "startup.loadingProjectIndex": "正在加载项目与会话索引",
    "startup.connectingService": "正在连接 Vulcan Core Service",
    "startup.loadingStep": "正在加载 {step}",
    "startup.loadingWorkspace": "正在加载 Workspace",
    "startup.downloadProgress": "下载进度",
    "startup.failed": "启动失败：{error}",
    "workspaceFiles.copyContentSuccess": "已复制文件内容。",
    "workspaceFiles.noProjectDirectory": "当前会话没有关联项目目录。",
    "workspaceFiles.noMatchedFiles": "没有匹配的文件。",
    "workspaceFiles.noDisplayableFiles": "暂无可显示文件。",
    "workspaceFiles.filterPlaceholder": "筛选文件...",
    "workspaceFiles.loading": "正在加载。",
    "workspaceFiles.loadingTree": "正在加载文件树。",
    "workspaceFiles.filterRules": "文件过滤规则",
    "workspaceFiles.hiddenRules": "隐藏规则",
    "workspaceFiles.ignoreHiddenFiles": "忽略隐藏文件",
    "workspaceFiles.ignoreDotNames": "忽略点开头名称",
    "workspaceFiles.useIgnoreRules": "使用 .ignore 规则",
    "workspaceFiles.useGitignoreRules": "使用 .gitignore 规则",
    "workspaceFiles.filterNonPreviewFiles": "过滤非代码、图片、视频文件",
    "workspaceFiles.fileActions": "文件操作",
    "workspaceFiles.copyPath": "复制路径",
    "workspaceFiles.copyRelativePath": "复制相对路径",
    "workspaceFiles.copyFileContent": "复制文件内容",
    "workspaceFiles.filterFiles": "筛选文件",
    "workspaceFilePreview.openFile": "打开文件",
    "workspaceFilePreview.emptyHint": "从文件树中选择代码或媒体文件。",
    "workspaceFilePreview.loading": "正在加载预览。",
    "workspaceFilePreview.unsupportedTitle": "暂不支持预览",
    "workspaceFilePreview.unsupportedHint": "该文件会显示在文件树中，但不会自动读取内容。",
    "workspaceFilePreview.noContent": "暂无预览内容。",
    "workspaceFilePreview.missingTitle": "文件已被删除或移动",
    "workspaceFilePreview.video": "视频",
    "workspaceFilePreview.image": "图片",
    "workspaceFilePreview.mediaBlockedTitle": "{media}文件不允许展示",
    "workspaceFilePreview.mediaBlockedHint": "是否允许展示当前{media}文件？允许后同类文件会直接预览。",
    "workspaceFilePreview.allowMedia": "允许显示{media}",
    "workspaceFilePreview.truncated": "已截断",
    "workspaceFilePreview.missingError": "文件已被删除或移动，无法预览：{path}",
    "workspaceFileTree.collapseDirectory": "收起文件目录",
    "workspaceFileTree.expandDirectory": "展开文件目录",
    "workspaceFileTree.expandAll": "全部展开",
    "workspaceFileTree.collapseAll": "全部折叠",
    "workspaceToolPanel.expandState": "展开状态",
    "workspaceToolPanel.memorySettingsMenu": "记忆设置菜单",
    "workspaceToolPanel.skillsSettingsMenu": "插件菜单",
    "workspaceToolPanel.agentsSettingsMenu": "智能体设置菜单",
    "workspaceToolPanel.menu": "菜单",
    "workspaceToolPanel.collapseToPill": "收起为胶囊",
    "workspaceToolPanel.status": "状态",
    "workspaceShell.skillsTitle": "插件",
    "workspaceShell.agentsTitle": "智能体设置",
    "workspaceShell.memoryTitle": "记忆设置",
    "workspaceShell.version": "版本 {version}",
    "workspacePanel.resizeRight": "调整右侧面板宽度",
    "workspaceContext.switchTabs": "切换右侧页签",
    "workspaceContext.searchTabs": "搜索标签页",
    "workspaceContext.searchTabsPlaceholder": "搜索标签页...",
    "workspaceContext.clearTabSearch": "清空标签页搜索",
    "workspaceContext.openTabs": "打开的标签页",
    "workspaceContext.closeTab": "关闭{title}",
    "workspaceContext.openRightPage": "打开右侧页面",
    "workspaceContext.openPage": "打开页面",
    "workspaceContext.launcher.filesDescription": "浏览项目文件",
    "workspaceContext.launcher.reviewDescription": "查看代码更改",
    "workspaceContext.launcher.memoryDescription": "查看会话记忆",
    "workspaceContext.loadingReviewPanel": "正在加载审核面板。",
    "workspaceContext.loadingFilesPanel": "正在加载文件面板。",
    "workspaceMemory.handleDynamicMemory": "处理动态记忆",
    "workspaceMemory.expiredRegionTitle": "已过期",
    "workspaceMemory.expiredRegionDescription": "以下记忆等待上下文压缩后清理",
    "workspaceMemory.suppressForSession": "仅本会话不再注入",
    "workspaceMemory.permanentDeleteInvalid": "永久删除无效记忆",
    "workspaceMemory.viewDetails": "查看详情",
    "workspaceMemory.turnId": "对话编号 {turnId}",
    "workspaceMemory.loadingTurn": "正在查询完整 TURN 记录...",
    "workspaceMemory.lookupFailed": "查询失败：{error}",
    "workspaceMemory.user": "用户：{content}",
    "workspaceMemory.assistant": "助手：{content}",
    "workspaceMemory.viewDetailsWithTurn": "查看详情 {turnId}",
    "workspaceMemory.handleMemoryWithId": "处理记忆 {memoryId}",
    "workspaceMemory.suppressedNotice": "已在当前会话中隐藏该动态记忆。",
    "workspaceMemory.suppressFailed": "隐藏失败：{error}",
    "workspaceMemory.deletedNotice": "记忆已删除。",
    "workspaceMemory.deletedWithoutId": "删除已执行，但服务端未返回已删除 ID。",
    "workspaceMemory.deleteFailed": "删除失败：{error}",
    "workspaceReview.refreshGitDiffFailed": "刷新 Git diff 失败：{error}",
    "workspaceReview.noProjectPathForGit": "当前会话没有关联项目路径，无法执行 Git 操作。",
    "workspaceReview.gitOperationFailed": "Git 操作失败：{error}",
    "workspaceReview.fileDetailTimeout": "加载文件 diff 详情超时：{error}",
    "workspaceReview.fileDetailFailed": "加载文件 diff 详情失败：{error}",
    "workspaceReview.noProjectPathForRefresh": "当前会话没有关联项目路径，无法刷新 Git diff。",
    "workspaceReview.fileDetailLoading": "正在加载文件 diff 详情。",
    "workspaceReview.hunkExpanding": "正在展开 hunk 上下文。",
    "workspaceReview.hunkDirectionUp": "向上",
    "workspaceReview.hunkDirectionDown": "向下",
    "workspaceReview.hunkDirectionBoth": "双向",
    "workspaceReview.hunkExpandingSpecific": "正在{direction}展开 hunk {index} 上下文。",
    "workspaceReview.fileDetailTimeoutError": "单文件 diff 详情加载超过 {seconds} 秒。",
    "workbenchConversation.unpinned": "已取消置顶对话：{conversation}",
    "workbenchConversation.pinned": "已置顶对话：{conversation}",
    "workbenchConversation.archived": "已归档对话：{conversation}",
    "workbenchConversation.noProject": "没有找到这个对话所属的项目。",
    "workbenchConversation.copiedWorkspace": "已复制工作目录。",
    "workbenchConversation.copiedTitle": "已复制标题。",
    "workbenchConversation.copiedSessionId": "已复制会话 ID。",
    "workbenchConversation.nameEmpty": "名称不能为空。",
    "workbenchConversation.projectRenamed": "已重命名项目：{project}",
    "workbenchConversation.projectNameRestored": "已还原项目名称：{project}",
    "workbenchConversation.conversationRenamed": "已重命名对话：{conversation}",
    "workbenchConversation.archivedCount": "已归档 {count} 个对话。",
    "workbenchConversation.archivedAndRemovedProject": "已归档并移除项目：{project}",
    "workbenchAsk.submitted": "已提交 ask 回答。",
    "workbenchAsk.cancelled": "已取消 ask 请求。",
    "thinkingDisplay.popupOnly": "弹窗显示",
    "thinkingDisplay.inlineOutput": "直接输出",
    "thinkingDisplay.firstInlineWithPopup": "首段输出+弹窗",
    "thinkingDisplay.firstInlineOnly": "仅首段输出",
    "ask.title": "ask 需要你的确认",
    "ask.description": "当前 AI 调用正在等待你的回答，提交后会继续执行。",
    "ask.action.confirmSubmit": "确认提交",
    "ask.action.confirmReject": "确认终止",
    "ask.action.submit": "提交",
    "ask.action.continue": "继续",
    "ask.action.ignore": "忽略",
    "ask.action.backToEdit": "返回修改",
    "ask.action.processing": "处理中...",
    "ask.progress.previous": "上一题",
    "ask.progress.next": "下一题",
    "ask.option.selected": "已选",
    "ask.confirm.submitTitle": "确认提交？",
    "ask.confirm.rejectTitle": "确认终止当前会话？",
    "ask.confirm.submitBody": "按 Enter 确认提交，按 Esc 返回修改。",
    "ask.confirm.rejectBody": "终止后等待中的 ask 工具调用会失败结束，按 Esc 返回修改。",
    "ask.custom.textPlaceholder": "请输入回答",
    "ask.custom.adjustCodex": "否，请告知 Codex 如何调整",
    "ask.custom.description": "输入自定义回答。支持自动换行，按 Shift+Enter 手动换行。",
    "ask.validation.requiredAnswer": "请先填写“{header}”的回答。",
    "ask.validation.requiredOther": "请先填写“{header}”的其他回答。",
    "ask.validation.requiredSelectionOrOther": "请先选择“{header}”的答案或填写其他回答。",
    "terminalStop.title": "确认强制终止终端会话？",
    "terminalStop.description": "该操作会停止正在运行的终端进程，请确认当前会话不再需要继续输出。",
    "terminalStop.stopping": "正在终止",
    "terminalStop.confirm": "确认终止",
    "conversation.bodyLoadFailed": "会话正文加载失败：{error}",
    "conversation.emptyLoaded": "当前会话正文尚未载入或没有可见消息。",
    "conversation.emptyWaiting": "尚未选择会话，等待工作区数据载入。",
    "conversation.detail.copy": "复制详情",
    "conversation.detail.close": "关闭详情",
    "conversation.userPrompt.expand": "显示更多",
    "conversation.userPrompt.collapse": "收起",
    "conversation.userPrompt.copy": "复制用户提示词",
    "conversation.userPrompt.edit": "编辑用户提示词",
    "conversation.subagent.schedule": "智能体信息",
    "conversation.subagent.call": "创建智能体",
    "conversation.subagent.calling": "创建智能体",
    "conversation.subagent.state.running": "运行中",
    "conversation.subagent.state.stopped": "已停止",
    "conversation.subagent.state.failed": "失败",
    "conversation.subagent.state.completed": "待命",
    "conversation.subagent.detail.createdTask": "已创建智能体任务",
    "conversation.subagent.detail.name": "名称",
    "conversation.subagent.detail.description": "描述",
    "conversation.subagent.detail.prompt": "提示词",
    "conversation.subagent.detail.effectivePrompt": "执行提示词",
    "conversation.subagent.detail.debug": "调试信息",
    "conversation.subagent.detail.empty": "无内容。",
    "conversation.subagent.detail.message": "向智能体发送消息",
    "conversation.subagent.detail.wait": "等待智能体消息",
    "conversation.subagent.detail.waitPending": "等待中",
    "conversation.subagent.detail.waitCompleted": "已收到智能体消息",
    "conversation.subagent.detail.waitInterrupted": "等待已被打断",
    "conversation.subagent.detail.waitTimedOut": "等待超时",
    "conversation.subagent.detail.templateCollection": "查询智能体模板列表",
    "conversation.subagent.detail.statusCollection": "查询智能体状态列表",
    "conversation.subagent.detail.feedback": "智能体返回处理结果",
    "conversation.subagent.detail.openFeedback": "点击展开",
    "conversation.subagent.detail.count": "数量",
    "conversation.subagent.detail.completed": "是否完成",
    "conversation.subagent.detail.timeout": "超时时间",
    "conversation.subagent.detail.noAgents": "没有可见智能体。",
    "conversation.subagent.detail.noOpenAgents": "当前无未关闭的智能体。",
    "conversation.subagent.enabled": "已启用",
    "conversation.subagent.disabled": "已停用",
    "conversation.subagent.detail.agent": "智能体",
    "conversation.subagent.detail.status": "状态",
    "conversation.subagent.detail.unknown": "未知",
    "conversation.subagent.detail.result": "返回内容",
    "conversation.subagent.detail.noResult": "没有返回内容。",
    "conversation.subagent.detail.target": "目标",
    "conversation.subagent.detail.queued": "是否排队",
    "conversation.subagent.detail.deliveryMode": "投递模式",
    "conversation.subagent.detail.targetSessions": "目标会话",
    "conversation.subagent.detail.note": "说明",
    "conversation.subagent.detail.closeControl": "关闭智能体",
    "conversation.subagent.detail.interruptControl": "停止智能体",
    "conversation.subagent.detail.stoppedCount": "已停止数量",
    "conversation.subagent.detail.stoppedSessions": "已停止会话",
    "conversation.subagent.detail.closedCount": "已关闭数量",
    "conversation.subagent.detail.closedAgents": "已关闭智能体",
    "conversation.subagent.detail.clearedMessages": "已清理消息",
    "conversation.subagent.detail.yes": "是",
    "conversation.subagent.detail.no": "否",
    "conversation.subagent.closeTask": "结束智能体",
    "conversation.subagent.loadingTranscript": "正在加载智能体消息",
    "conversation.subagent.closeConfirmTitle": "结束这个智能体？",
    "conversation.subagent.closeConfirmDescription": "将结束「{task}」的智能体生命周期并清理待处理消息。结束后不能继续发送消息。",
    "conversation.subagent.closeConfirm": "确认结束",
    "conversation.subagent.closing": "正在结束",
    "conversation.subagent.stopConfirmTitle": "停止这个智能体？",
    "conversation.subagent.stopConfirmDescription": "将单独停止「{task}」的当前工作，不会关闭智能体，也不会停止主会话或其他智能体。",
    "conversation.subagent.stopConfirm": "确认停止",
    "conversation.subagent.stopping": "正在停止",
    "conversation.diff.loadingPanel": "正在加载 Git Diff 审核面板。",
    "conversation.diff.parsingWorker": "正在通过 Worker 解析 diff 内容。",
    "conversation.diff.parseFailed": "Diff 解析失败：{error}",
    "conversation.diff.unstructuredFallback": "当前 diff 内容无法结构化解析，已保留原始文本供复制。",
    "sessionTools.expand": "展开会话工具",
    "sessionTools.collapse": "折叠会话工具",
    "sessionTools.changes": "更改",
    "sessionTools.progress": "进程",
    "sessionTools.terminal": "终端",
    "sessionTools.menu": "会话工具菜单",
    "terminalSession.openScreen": "打开终端屏幕",
    "terminalSession.noScreen": "暂无终端屏幕",
    "terminalSession.forceStop": "强制终止",
    "terminalSession.forceStopSession": "强制终止终端会话",
    "terminalSession.emptyScreen": "暂无终端屏幕内容",
    "terminalSession.tooltip.status": "状态",
    "terminalSession.tooltip.command": "命令",
    "terminalSession.tooltip.cwd": "目录",
    "terminalSession.tooltip.shell": "Shell",
    "terminalSession.tooltip.backend": "后端",
    "terminalSession.tooltip.screenFrame": "屏幕帧",
    "terminalSession.tooltip.exitCode": "退出码",
    "terminalSession.tooltip.error": "错误",
    "terminalSession.tooltip.output": "输出",
    "terminalSession.tooltip.session": "会话",
    "terminalState.running": "运行中",
    "terminalState.waiting": "等待输入",
    "terminalState.success": "已完成",
    "terminalState.failed": "执行失败",
    "terminalState.stopped": "已停止",
    "terminalState.lost": "已丢失",
    "terminalState.unknown": "未知",
    "toolPresentation.output.loading": "正在加载输出...",
    "toolPresentation.output.empty": "无输出",
    "toolPresentation.exitCode": "退出代码",
    "toolPresentation.command.running": "执行中",
    "toolPresentation.command.failed": "执行失败",
    "toolPresentation.command.stopped": "已停止",
    "toolPresentation.command.completed": "已执行",
    "toolPresentation.command.start.failed": "启动失败",
    "toolPresentation.command.read.failed": "读取失败",
    "toolPresentation.command.send.failed": "发送失败",
    "toolPresentation.command.write.failed": "写入失败",
    "toolPresentation.command.stop.failed": "停止失败",
    "toolPresentation.command.start.running": "启动中",
    "toolPresentation.command.read.running": "读取中",
    "toolPresentation.command.send.running": "发送中",
    "toolPresentation.command.write.running": "写入中",
    "toolPresentation.command.stop.running": "停止中",
    "toolPresentation.command.start.completed": "已启动",
    "toolPresentation.command.list.completed": "已列出",
    "toolPresentation.command.read.completed": "已读取",
    "toolPresentation.command.send.completed": "已发送",
    "toolPresentation.command.write.completed": "已写入",
    "toolPresentation.command.stop.completed": "已停止",
    "toolPresentation.command.heading.start": "启动命令",
    "toolPresentation.command.heading.read": "读取输出",
    "toolPresentation.command.heading.send": "发送内容",
    "toolPresentation.command.heading.write": "写入内容",
    "toolPresentation.command.heading.stop": "停止会话",
    "toolPresentation.command.session": "命令会话",
    "toolPresentation.command.lost": "实时状态已丢失",
    "toolPresentation.command.current": "当前",
    "toolPresentation.command.currentState": "当前{state}",
    "toolPresentation.lua.title": "执行 Lua 代码",
    "toolPresentation.lua.running": "执行中",
    "toolPresentation.lua.failed": "执行失败",
    "toolPresentation.lua.stopped": "已停止",
    "toolPresentation.lua.completed": "已执行",
    "toolPresentation.lua.action.eval": "执行Lua代码",
    "toolPresentation.lua.action.invalid": "Lua会话操作失败",
    "toolPresentation.lua.action.list": "列出Lua会话",
    "toolPresentation.lua.action.start": "启动Lua持久会话",
    "toolPresentation.lua.action.status": "读取Lua会话状态",
    "toolPresentation.lua.action.close": "关闭Lua会话",
    "toolPresentation.lua.oneShot": "隔离执行",
    "toolPresentation.lua.persistent": "持久会话",
    "toolPresentation.lua.session": "会话",
    "toolPresentation.lua.ttl": "闲置时限",
    "toolPresentation.lua.elapsed": "耗时",
    "toolPresentation.lua.code": "Lua 代码",
    "toolPresentation.lua.arguments": "结构化参数",
    "toolPresentation.lua.output": "执行结果",
    "toolPresentation.lua.argumentName": "参数名字",
    "toolPresentation.lua.argumentValue": "参数值",
    "toolPresentation.lua.sessionName": "名称",
    "toolPresentation.lua.sessionId": "会话 ID",
    "toolPresentation.lua.sessionState": "状态",
    "toolPresentation.lua.sessionTtl": "闲置时限",
    "toolPresentation.lua.sessionExpiresAt": "到期时间",
    "toolPresentation.lua.sessionActive": "运行中",
    "toolPresentation.lua.sessionClosed": "已关闭",
    "toolPresentation.lua.sessionExpired": "已过期",
    "toolPresentation.lua.sessionFailed": "异常",
    "toolPresentation.lua.noSessions": "当前没有 Lua 持久会话。",
    "toolPresentation.lua.closeSucceeded": "Lua 会话 `{session}` 已关闭。",
    "toolPresentation.ask.running": "提问中",
    "toolPresentation.ask.ignored": "已忽略",
    "toolPresentation.ask.completed": "已提问",
    "toolPresentation.ask.questionLabel": "问题：",
    "toolPresentation.ask.answerLabel": "回答：",
    "toolPresentation.ask.waitingAnswer": "等待用户回答",
    "toolPresentation.file.count": "{count} 个文件",
    "toolPresentation.file.multiChange": "多文件变更",
    "toolPresentation.file.empty": "无可展示内容",
    "toolPresentation.file.create.running": "正在写入",
    "toolPresentation.file.modify.running": "正在编辑",
    "toolPresentation.file.delete.running": "正在删除",
    "toolPresentation.file.mixed.running": "正在变更",
    "toolPresentation.file.create.failed": "写入失败",
    "toolPresentation.file.modify.failed": "编辑失败",
    "toolPresentation.file.delete.failed": "删除失败",
    "toolPresentation.file.mixed.failed": "变更失败",
    "toolPresentation.file.create.preview": "预览写入",
    "toolPresentation.file.modify.preview": "预览编辑",
    "toolPresentation.file.delete.preview": "预览删除",
    "toolPresentation.file.mixed.preview": "预览变更",
    "toolPresentation.file.create.completed": "已写入",
    "toolPresentation.file.modify.completed": "已编辑",
    "toolPresentation.file.delete.completed": "已删除",
    "toolPresentation.file.mixed.completed": "已变更",
    "toolPresentation.markdownOutput.running": "查询中",
    "toolPresentation.markdownOutput.failed": "查询失败",
    "toolPresentation.markdownOutput.stopped": "已停止",
    "toolPresentation.markdownOutput.completed": "已查询",
    "toolPresentation.fileList.title": "查询文件列表",
    "toolPresentation.fileRead.title": "读取文件",
    "toolPresentation.codekit.astTree.title": "列出 AST 树",
    "toolPresentation.codekit.astDetail.title": "查看 AST 详情",
    "toolPresentation.codekit.rg.title": "RG增强探索",
    "toolPresentation.codekit.nodeSource.title": "获取节点源码",
    "markdown.wrap.enable": "开启自动换行",
    "markdown.wrap.disable": "关闭自动换行",
    "markdown.copyCode": "复制代码",
    "markdown.copied": "已复制",
    "mediaPlayer.maximize": "最大化播放器",
    "mediaPlayer.restore": "还原播放器",
    "mediaPlayer.close": "关闭播放器",
    "mediaPlayer.fitWindow": "自适应窗口",
    "mediaPlayer.originalSize": "原始尺寸",
    "mediaPlayer.zoomOut": "缩小图片",
    "mediaPlayer.zoomIn": "放大图片",
    "media.openVideoPlayer": "打开视频播放器",
    "settings.backToWorkspace": "返回工作台",
    "memory.scope": "作用域",
    "memory.scope.global": "全局配置",
    "memory.scope.project": "项目配置",
    "memory.project": "项目",
    "memory.action.testConnection": "测试连接",
    "memory.action.refreshUsersProjects": "刷新用户/项目",
    "memory.vmm.action.running": "正在执行 VMM 动作：{action}",
    "memory.vmm.action.failed": "VMM 动作失败：{message}",
    "memory.vmm.project.loading": "正在加载项目 VMM 配置：{project}。",
    "memory.vmm.project.selected": "当前编辑项目：{project}。",
    "memory.vmm.save.global.running": "正在保存全局 VMM 配置。",
    "memory.vmm.save.project.running": "正在保存项目 VMM 配置。",
    "memory.vmm.save.global.completed": "已保存全局 VMM 配置。",
    "memory.vmm.save.project.completed": "已保存项目 VMM 配置。",
    "memory.vmm.save.failed": "保存 VMM 配置失败：{message}",
    "vmm.common.optional": "可选",
    "vmm.common.number": "数字",
    "vmm.common.enabled": "启用",
    "vmm.common.disabled": "关闭",
    "vmm.common.none": "无",
    "vmm.common.notConfigured": "未配置",
    "vmm.common.notSelected": "未选择",
    "vmm.common.running": "执行中",
    "vmm.overview.effectiveConfig": "有效配置",
    "vmm.overview.grpc": "gRPC：",
    "vmm.overview.recallPolicy": "召回策略",
    "vmm.overview.ttlTurns": "TTL：{turns} 回合",
    "vmm.overview.precheckAiTools": "前置召回：{precheck} · AI 工具：{tools}",
    "vmm.overview.writeback": "后置提交：{writeback}",
    "vmm.overview.runtime": "运行态",
    "vmm.overview.outboxProfiles": "outbox：{outbox} · 画像节点：{profiles}",
    "vmm.overview.completedTurns": "已完成回合：{turns}",
    "vmm.overview.summaryEntry": "摘要入口",
    "vmm.overview.noSummary": "暂无摘要",
    "vmm.runtime.title": "运行态",
    "vmm.runtime.subtitle": "当前会话的 VMM hook 与画像缓存。",
    "vmm.runtime.session": "会话",
    "vmm.runtime.profileNodes": "画像节点",
    "vmm.runtime.updatedAt": "更新时间",
    "vmm.runtime.noProfileBase": "暂无画像基点。",
    "vmm.result.title": "动作结果",
    "vmm.result.emptySummary": "尚未执行 VMM 动作。",
    "vmm.result.request": "请求",
    "vmm.result.response": "响应",
    "vmm.result.emptyHint": "点击左侧动作后会显示请求与响应。",
    "vmm.result.generic": "结果",
    "vmm.settings.grpcTarget": "gRPC 地址",
    "vmm.settings.apiKey": "API Key",
    "vmm.settings.receiveTimeout": "请求超时",
    "vmm.settings.handshakeTimeout": "握手超时",
    "vmm.settings.maxReceiveBytes": "最大接收字节",
    "vmm.settings.keepaliveInterval": "keepalive 间隔",
    "vmm.settings.keepaliveTimeout": "keepalive 超时",
    "vmm.settings.idleKeepalive": "空闲 keepalive",
    "vmm.settings.enableMemoryConnection": "启用记忆连接",
    "vmm.settings.exposeAiTools": "暴露 AI 专属工具",
    "vmm.settings.autoPrecheck": "自动前置召回",
    "vmm.settings.precheckPolicy": "前置记忆注入",
    "vmm.settings.writebackPolicy": "后置记忆提交",
    "vmm.settings.policy.inherit": "跟随上级",
    "vmm.settings.policy.enabled": "开启",
    "vmm.settings.policy.disabled": "关闭",
    "vmm.settings.userId": "用户 ID",
    "vmm.settings.projectId": "项目 ID",
    "vmm.settings.implicitMemoryTtl": "临时记忆 TTL",
    "vmm.settings.profileRefreshInterval": "画像刷新间隔",
    "vmm.settings.profileNodeLimit": "画像节点上限",
    "vmm.settings.injectionTag": "注入标签",
    "vmm.settings.injectionDescription": "注入说明",
    "vmm.rows.usersEmpty": "执行“刷新用户/项目”后会显示用户列表。",
    "vmm.rows.projectsEmpty": "执行“刷新用户/项目”后会显示项目列表。",
    "vmm.rows.profileNodesEmpty": "刷新画像节点后会在这里显示 active profile nodes。",
    "vmm.rows.bundleEmpty": "读取完整画像后会显示 bundle 内容。",
    "vmm.rows.toolResultEmpty": "工具调试结果会按动作显示。",
    "vmm.rows.unnamedUser": "未命名用户",
    "vmm.rows.unnamedProject": "未命名项目",
    "vmm.rows.deleted": "已删除",
    "vmm.rows.notFound": "未找到",
    "vmm.rows.vectorRows": "向量行 {count}",
    "vmm.profile.full": "完整画像",
    "vmm.profile.team": "团队画像",
    "vmm.profile.space": "空间画像",
    "vmm.profile.project": "项目画像",
    "vmm.profile.user": "用户画像",
    "vmm.action.deleteMemory": "删除记忆",
    "vmm.action.precheckProject": "预检项目",
    "vmm.action.confirmCreate": "确认创建",
    "vmm.action.deleteProject": "删除项目",
    "vmm.action.runMigrate": "执行迁移",
    "vmm.action.refreshNodes": "刷新节点",
    "vmm.action.applyInstruction": "应用指令",
    "vmm.action.loadProfileBundle": "读取完整画像",
    "vmm.action.saveGlobal": "保存到全局",
    "vmm.action.saveProject": "保存到项目",
    "vmm.action.resolveUser": "解析用户",
    "vmm.action.deleteUser": "删除用户",
    "vmm.section.project.title": "项目管理",
    "vmm.section.project.subtitle": "项目路径预检、创建确认、删除确认和迁移确认。",
    "vmm.section.profile.title": "画像中心",
    "vmm.section.profile.subtitle": "读取 active profile nodes，并向当前目标提交手工画像指令。",
    "vmm.section.bundle.title": "画像 Bundle 测试",
    "vmm.section.bundle.subtitle": "读取当前 user/project 的 FULL bundle，检查画像组合结果。",
    "vmm.section.tool.title": "工具调试",
    "vmm.section.tool.subtitle": "模拟 VMM AI 专属工具：检索记忆、展开 turn、主动写入记忆。",
    "vmm.section.memory.title": "记忆设置",
    "vmm.section.memory.subtitle": "配置记忆开关、AI 工具、临时 TTL、画像刷新和注入说明。",
    "vmm.section.transport.title": "gRPC 传输",
    "vmm.section.transport.subtitle": "连接地址、API key、超时、最大接收字节数与 keepalive 策略。",
    "vmm.section.user.title": "用户管理",
    "vmm.section.user.subtitle": "长期用户列表、解析用户引用、创建确认和删除确认。",
    "vmm.field.projectPath": "项目路径",
    "vmm.field.confirmDelete": "确认删除",
    "vmm.field.migrateSource": "迁移源项目",
    "vmm.field.migrateTarget": "迁移目标项目",
    "vmm.field.confirmMigrate": "确认迁移",
    "vmm.field.profileTarget": "画像目标",
    "vmm.field.profileInstruction": "手工画像指令",
    "vmm.field.searchQueries": "检索查询",
    "vmm.field.deleteReason": "删除原因",
    "vmm.field.writeAbstract": "主动写入摘要",
    "vmm.field.writeDetails": "主动写入详情",
    "vmm.field.userRef": "用户引用",
    "vmm.field.deleteConfirmationCode": "删除确认码",
    "vmm.placeholder.profileInstruction": "例如：降低某条偏好权重",
    "vmm.placeholder.multiPipe": "多条用 | 分隔",
    "vmm.placeholder.commaOrPipe": "用逗号或 | 分隔",
    "vmm.placeholder.deleteReason": "错误、过期或用户要求删除",
    "vmm.placeholder.writeAbstract": "记忆摘要",
    "vmm.placeholder.writeDetails": "完整内容",
    "vmm.placeholder.userRef": "ID 或用户名",
    "vmm.placeholder.deleteConfirmationCode": "首次删除后返回",
    "vmm.diagnostic.disabledByConfig": "已关闭",
    "vmm.diagnostic.missingGrpcTarget": "缺少 gRPC",
    "vmm.diagnostic.grpcOffline": "gRPC 离线",
    "vmm.diagnostic.missingUserId": "缺少用户",
    "vmm.diagnostic.missingProjectId": "缺少项目",
    "planArtifact.titleFallback": "计划",
    "planArtifact.copy": "复制 Markdown",
    "planArtifact.copied": "已复制 Markdown",
    "planArtifact.expand": "展开计划",
    "planArtifact.collapse": "折叠计划",
    "planArtifact.confirmAria": "计划确认操作",
    "planArtifact.noteAria": "计划操作补充内容",
    "planArtifact.notePlaceholder": "补充内容（可选）",
    "planArtifact.execute": "执行计划",
    "planArtifact.executeWithNote": "补充信息并执行计划",
    "planArtifact.defer": "暂不执行",
    "planArtifact.suggest": "补充计划内容",
    "runtime.memoryRecall": "✨ 正在记忆召回",
    "runtime.requesting": "✨ 正在请求",
    "runtime.toolCalling": "🔧 正在构造工具请求",
    "runtime.thinking": "✨ 正在思考",
    "runtime.thinkingOverlayDockRight": "切换到右侧紧凑思考面板",
    "runtime.thinkingOverlayRestoreCentered": "恢复底部居中思考面板",
    "settings.general": "通用设置",
    "settings.systemSettings": "系统设置",
    "settings.projects": "项目",
    "settings.providers": "模型",
    "settings.tools": "工具",
    "settings.diagnostics": "诊断",
    "settings.general.description": "运行时、远程访问与界面设置",
    "settings.projects.description": "工作区项目列表与绑定状态",
    "settings.providers.description": "Provider、模型、推理与工具能力",
    "settings.tools.description": "工具调用、命令会话与输出",
    "settings.diagnostics.description": "启动日志、错误与运行时事件",
    "settings.group.models": "模型设置",
    "settings.group.aiCapabilities": "AI 能力",
    "settings.modelProviders": "模型供应商",
    "settings.modelList": "模型列表",
    "settings.providerRetry": "请求重试",
    "settings.aiVendors": "能力供应商",
    "settings.aiWebSearch": "网页搜索",
    "settings.aiUnderstandImage": "图片理解",
    "settings.aiUnderstandVideo": "视频理解",
    "settings.aiUnderstandAudio": "音频理解",
    "settings.aiImageGeneration": "图片生成",
    "settings.aiVideoGeneration": "视频生成",
    "settings.aiSpeechSynthesis": "语音合成",
    "settings.aiMusicGeneration": "音乐生成",
    "settings.aiMusicCover": "音乐翻唱",
    "settings.mcpServers": "MCP 服务器",
    "settings.runtimeDiagnostics": "运行诊断",
    "settings.gitCommit": "Git 提交",
    "settings.promptComposer": "提示词组合",
    "settings.aboutLicenses": "关于与许可证",
    "settings.archivedRestore": "归档恢复",
    "settings.modelProviders.description": "对话模型供应商、当前模型和已启用模型状态。",
    "settings.modelList.description": "按供应商查看已启用模型，并核对当前工作台模型。",
    "settings.providerRetry.description": "配置普通错误与 429 限流的重试次数、等待间隔和递增规则。",
    "settings.aiVendors.description": "多个语义 AI 工具共用的能力供应商与路由目标。",
    "settings.aiWebSearch.description": "网页搜索能力的启用状态与可用目标。",
    "settings.aiUnderstandImage.description": "图片理解能力的启用状态与可用目标。",
    "settings.aiUnderstandVideo.description": "视频理解能力入口与接入状态。",
    "settings.aiUnderstandAudio.description": "音频理解能力入口与接入状态。",
    "settings.aiImageGeneration.description": "图片生成能力的启用状态与可用目标。",
    "settings.aiVideoGeneration.description": "视频生成能力的启用状态与可用目标。",
    "settings.aiSpeechSynthesis.description": "语音合成能力的启用状态与可用目标。",
    "settings.aiMusicGeneration.description": "音乐生成能力的启用状态与可用目标。",
    "settings.aiMusicCover.description": "音乐翻唱能力的启用状态与可用目标。",
    "settings.mcpServers.description": "MCP Runtime、服务连接、工具、提示词与资源状态。",
    "settings.runtimeDiagnostics.description": "启动链路、运行时模块与诊断日志。",
    "settings.diagnostics.logs": "诊断日志",
    "settings.diagnostics.startupStatus": "启动状态",
    "settings.diagnostics.waitingStatus": "等待状态更新",
    "settings.diagnostics.runtimeModules": "运行时模块",
    "settings.gitCommit.description": "提交弹窗的新分支命名、自动总结模型与额外提示词。",
    "settings.gitCommit.active": "已生效",
    "settings.gitCommit.autoSummary": "自动总结",
    "settings.gitCommit.summaryModel": "提交总结模型",
    "settings.gitCommit.followSessionModel": "跟随提交所属会话模型",
    "settings.gitCommit.chooseSummaryModel": "选择提交总结模型",
    "settings.gitCommit.resetSummaryModel": "恢复跟随会话",
    "settings.gitCommit.followSessionModelHelp": "不固定模型时，自动总结会使用触发提交动作的会话当前模型。",
    "settings.gitCommit.fixedSummaryModelHelp": "已固定模型时，自动总结会优先使用该模型；模型不可用时后端会拒绝保存或提示重新选择。",
    "settings.gitCommit.fixedSummaryModelUnavailable": "当前快照中找不到该固定模型，请重新选择或恢复跟随会话。",
    "settings.gitCommit.summaryExtraPrompt": "额外总结提示词",
    "settings.gitCommit.summaryExtraPromptPlaceholder": "例如：提交信息必须使用中文；首行不超过 50 字；正文按要点分行。",
    "settings.gitCommit.summaryExtraPromptHelp": "该内容会追加到固定英文基础提示词之后，用于表达你的提交信息习惯。",
    "gitCommitDialog.generateEmpty": "未生成提交信息。",
    "gitCommitDialog.generated": "已生成提交信息，约",
    "gitCommitDialog.generatedTruncated": "已生成提交信息，diff 已裁剪，约",
    "gitCommitDialog.actionDone": "Git 操作已完成。",
    "gitCommitDialog.pushDone": "Git 推送已完成。",
    "gitCommitDialog.generating": "生成中",
    "gitCommitDialog.preparingCommitContent": "正在整理提交内容",
    "gitCommitDialog.generatingCommitMessage": "正在生成提交提示词",
    "gitCommitDialog.committing": "正在提交",
    "gitCommitDialog.pushingDetailed": "正在推送",
    "gitCommitDialog.running": "执行中",
    "gitCommitDialog.pushing": "推送中",
    "gitCommitDialog.toolsTitle": "GIT 工具",
    "gitCommitDialog.changes": "更改",
    "gitCommitDialog.open": "提交或推送",
    "gitCommitDialog.title": "提交或推送",
    "gitCommitDialog.description": "选择提交目标、填写提交信息，并根据 Git 变更状态启用提交操作。",
    "gitCommitDialog.currentBranch": "当前分支",
    "gitCommitDialog.newBranch": "新分支",
    "gitCommitDialog.recommendedPrefix": "推荐：",
    "gitCommitDialog.noChanges": "无更改",
    "gitCommitDialog.branchName": "新分支名称",
    "gitCommitDialog.message": "提交信息",
    "gitCommitDialog.messagePlaceholder": "提交信息（留空将自动生成）...",
    "gitCommitDialog.includeUnstaged": "包含未暂存的更改",
    "gitCommitDialog.commit": "提交",
    "gitCommitDialog.commitAndPush": "提交并推送",
    "gitCommitDialog.push": "推送",
    "gitCommitDialog.commitTo": "提交到",
    "settings.promptComposer.description": "输入区 slash 指令、附件能力与推理选择。",
    "settings.aboutLicenses.description": "应用版本、许可证入口与合规说明。",
    "settings.archivedRestore.description": "归档项目与会话恢复入口。",
    "skills.action.refresh": "刷新",
    "skills.action.refreshing": "刷新中",
    "skills.action.installLua": "安装 LuaSkills",
    "skills.action.install": "安装",
    "skills.action.installing": "安装中",
    "skills.action.cancel": "取消",
    "skills.action.details": "详情",
    "skills.action.tools": "工具",
    "skills.action.action": "操作",
    "skills.action.enable": "启用",
    "skills.action.disable": "停用",
    "skills.action.update": "更新",
    "skills.action.delete": "删除",
    "skills.status.enabled": "已启用",
    "skills.status.stopped": "已停用",
    "skills.filter.searchLabel": "搜索列表",
    "skills.filter.searchPlaceholder": "搜索名称、介绍、标识或来源",
    "skills.filter.statusLabel": "按状态筛选",
    "skills.filter.allStatuses": "全部状态",
    "skills.filter.issues": "存在问题",
    "skills.filter.sourceLabel": "按来源筛选",
    "skills.filter.allSources": "全部来源",
    "skills.filter.empty": "没有符合当前筛选条件的项目。",
    "skills.label.system": "系统",
    "skills.label.user": "用户",
    "skills.label.source": "来源",
    "skills.label.explicit": "显式",
    "skills.label.implicit": "隐式",
    "skills.label.closed": "关闭",
    "skills.label.currentStatus": "当前状态",
    "skills.label.installLayer": "安装层级",
    "skills.label.installManagement": "安装管理",
    "skills.label.runtimeEntries": "运行时入口",
    "skills.label.runtimeTools": "运行时工具",
    "skills.label.managedInstall": "受管安装",
    "skills.label.unmanagedInstall": "非受管安装",
    "skills.label.scope": "作用域",
    "skills.label.skillFile": "技能文件",
    "skills.label.skillDir": "技能目录",
    "skills.source.unknown": "未记录来源",
    "skills.sidebar.luaDetail": "{count} 个技能包",
    "skills.navigation.list": "LuaSkills 技能",
    "skills.navigation.plugins": "插件管理",
    "skills.navigation.rtkSettings": "RTK 设置",
    "skills.navigation.luaSettings": "LuaSkills 设置",
    "skills.luaSettings.description": "这些设置归 Vulcan 所有，并同时用于会话与服务两个 LuaSkills 引擎。",
    "skills.luaSettings.restartRequired": "已保存，等待重启生效",
    "skills.luaSettings.restartHint": "RunLua AI 工具开关保存后立即生效；VM 池、网络、运行时与能力参数仍需重启 Vulcan Code。",
    "skills.luaSettings.saved": "Vulcan LuaSkills 配置已保存。",
    "skills.luaSettings.save": "保存 LuaSkills 配置",
    "skills.luaSettings.saving": "正在保存…",
    "skills.luaSettings.aiTools": "AI 工具",
    "skills.luaSettings.aiToolsHint": "仅控制 AI 可见工具，不会关闭 LuaSkills 内部运行时或插件使用的 Lua 会话。",
    "skills.luaSettings.enableRunLua": "runlua",
    "skills.luaSettings.enableRunLuaSession": "runlua-session",
    "skills.luaSettings.runLuaTooltip": "执行一次隔离的 Lua 代码，适合单次计算、转换和脚本处理；调用结束后不保留 Lua 状态。",
    "skills.luaSettings.runLuaSessionTooltip": "启用持久 Lua 会话，可在多次调用之间保留变量、模块和中间结果；对应 AI 工具 runlua_session。",
    "skills.luaSettings.mainPool": "主 VM 池",
    "skills.luaSettings.runLuaPool": "隔离 runLua VM 池",
    "skills.luaSettings.poolHint": "最小值决定预热容量，最大值限制并发 VM 数，空闲时间控制多余 VM 的回收。",
    "skills.luaSettings.minSize": "最小 VM 数",
    "skills.luaSettings.maxSize": "最大 VM 数",
    "skills.luaSettings.idleTtl": "空闲回收（秒）",
    "skills.luaSettings.toolCache": "工具缓存",
    "skills.luaSettings.maxEntries": "最大条目数",
    "skills.luaSettings.defaultTtl": "默认 TTL（秒）",
    "skills.luaSettings.maxTtl": "最大 TTL（秒）",
    "skills.luaSettings.network": "网络与来源",
    "skills.luaSettings.allowDownloads": "允许下载技能与依赖",
    "skills.luaSettings.githubBaseUrl": "GitHub 网站基址",
    "skills.luaSettings.githubApiBaseUrl": "GitHub API 基址",
    "skills.luaSettings.skillHubBaseUrl": "LuaSkills Hub 基址",
    "skills.luaSettings.privateUrlInstall": "允许受信系统操作安装私有 URL 清单",
    "skills.luaSettings.privateAllowlist": "私有来源允许列表",
    "skills.luaSettings.privateAllowlistHint": "每行填写一个允许的 URL 前缀；空行会在保存时移除。",
    "skills.luaSettings.capabilities": "运行时能力",
    "skills.luaSettings.defaultEncoding": "默认文本编码",
    "skills.luaSettings.autoEncoding": "自动（跟随运行时）",
    "skills.luaSettings.skillManagementBridge": "允许 Lua 调用宿主技能管理桥接",
    "skills.luaSettings.managedIoCompat": "使用托管 IO 兼容层替换原生全局 IO",
    "skills.luaSettings.managedRuntime": "Python / Node 受管运行时",
    "skills.luaSettings.managedRuntimeHint": "这些参数控制每个引擎的 Worker、持久会话与调用默认值，保存后重启生效。",
    "skills.luaSettings.managedRuntimeStatusReady": "已就绪",
    "skills.luaSettings.managedRuntimeStatusInstalled": "本次运行已安装",
    "skills.luaSettings.managedRuntimeStatusUnsupported": "当前平台不支持",
    "skills.luaSettings.managedRuntimeStatusUnavailable": "安装不可用",
    "skills.luaSettings.managedRuntimeRetrying": "正在重试下载…",
    "skills.luaSettings.managedRuntimeRetryCompleted": "受管运行时初始化已完成，请查看最新状态。",
    "skills.luaSettings.pythonRuntime": "Python 运行时",
    "skills.luaSettings.nodeRuntime": "Node.js 运行时",
    "skills.luaSettings.managedRuntimeInstalled": "已安装",
    "skills.luaSettings.managedRuntimeReinstall": "重新安装",
    "skills.luaSettings.managedRuntimePreparing": "正在准备安装…",
    "skills.luaSettings.combinedRuntime": "Python / Node 运行时",
    "skills.luaSettings.managedRuntimeNormal": "正常",
    "skills.luaSettings.managedWorkerPoolMax": "单环境最大 Worker 数",
    "skills.luaSettings.managedWorkerIdleTtl": "Worker 空闲回收（秒）",
    "skills.luaSettings.managedSessionLimit": "单引擎持久会话上限",
    "skills.luaSettings.managedSessionBuffer": "单输出流默认缓冲字节数",
    "skills.luaSettings.managedInvokeTimeout": "默认调用超时（毫秒）",
    "skills.luaSettings.unlimited": "无限制",
    "skills.luaSettings.validationPositive": "容量、TTL 与空闲时间必须是大于 0 的整数。",
    "skills.luaSettings.validationPoolOrder": "每个 VM 池的最大 VM 数不能小于最小 VM 数。",
    "skills.luaSettings.validationCacheOrder": "最大缓存 TTL 不能小于默认缓存 TTL。",
    "skills.sidebar.agentDetail": "{count} 个技能",
    "skills.sidebar.pluginDetail": "{count} 个 Vulcan 插件",
    "skills.header.luaDetail": "总技能数 {total} · 显示工具数 {visible} · 隐式工具数 {implicit}",
    "skills.header.pluginDetail": "Vulcan 插件 {total} · 已启用 {enabled}",
    "skills.header.rtkDetail": "命令路由、Git 上下文压缩与 Token 节省统计",
    "agents.header.detail": "总智能体 {total} · 已启用 {enabled} · 加载警告 {warnings}",
    "skills.lua.emptyTitle": "等待 LuaSkills 目录",
    "skills.lua.emptyDetail": "启动完成或刷新后会显示真实技能包。",
    "skills.lua.installTitle": "安装 LuaSkills",
    "skills.lua.installDescription": "从 GitHub 仓库或 URL 安装一个 Lua Skill 包。",
    "skills.lua.sourceType": "来源类型",
    "skills.lua.installSource": "安装来源",
    "skills.lua.placeholderGithub": "LuaSkills/vulcan-codekit",
    "skills.lua.placeholderUrl": "https://example.com/skill-source.yaml",
    "skills.lua.detailsTitle": "Lua Skill 详情",
    "skills.lua.detailsDescription": "查看 Lua Skill 包详情。",
    "skills.lua.toolsTitle": "LuaSkills 工具",
    "skills.lua.toolsDescription": "管理 Lua Skill 工具可见性。",
    "skills.lua.toolsEmpty": "当前技能尚未解析到可管理的运行时工具。",
    "skills.lua.toolModeAria": "{name} 工具启用模式",
    "skills.lua.toolExplicit": "显式启用",
    "skills.lua.toolImplicit": "隐式启用",
    "skills.lua.toolDisabled": "关闭工具",
    "skills.lua.syncing": "同步中",
    "skills.luaConfig.title": "技能包配置",
    "skills.luaConfig.description": "按照技能包声明管理配置。",
    "skills.luaConfig.loading": "正在加载技能包配置…",
    "skills.luaConfig.empty": "当前技能包没有声明配置项。",
    "skills.luaConfig.complete": "配置完整",
    "skills.luaConfig.incomplete": "配置不完整",
    "skills.luaConfig.required": "必填",
    "skills.luaConfig.sensitive": "敏感",
    "skills.luaConfig.restartRequired": "修改后需重启",
    "skills.luaConfig.deprecated": "已弃用",
    "skills.luaConfig.secretConfigured": "已配置；留空不会修改当前值。",
    "skills.luaConfig.preserveSecret": "保留当前敏感值",
    "skills.luaConfig.selectValue": "请选择",
    "skills.luaConfig.deleteValue": "删除显式配置值",
    "skills.luaConfig.reload": "重新加载最新配置",
    "skills.luaConfig.advanced": "高级配置",
    "skills.luaConfig.orphaned": "遗留配置键",
    "skills.luaConfig.externalChange": "检测到外部配置变更，已加载最新修订并保留未提交草稿。",
    "skills.luaConfig.revisionConflict": "配置已被其他运行时修改，已加载最新修订并保留未提交草稿，请确认后重新保存。",
    "skills.luaConfig.saving": "保存中…",
    "skills.luaConfig.save": "保存配置",
    "skills.plugins.emptyTitle": "暂无 Vulcan 插件",
    "skills.plugins.emptyDetail": "随包的 Vulcan 插件会显示在这里。",
    "skills.plugins.loadingPage": "正在加载插件页面…",
    "skills.plugins.externalUrlConfirmation": "此外部链接不在白名单中，确认信任后仍可打开。",
    "skills.plugins.externalUrlOpenAnyway": "仍然打开",
    "skills.plugins.externalUrlOpenFailed": "打开外部链接失败",
    "skills.plugins.unavailable": "不可用",
    "skills.plugins.enabling": "正在启用",
    "skills.plugins.draining": "正在排空会话",
    "skills.plugins.cleanupWarning": "已停用（清理警告）",
    "skills.plugins.unsupportedPlatform": "当前平台不支持",
    "skills.plugins.retryCleanup": "重试清理",
    "skills.plugins.systemKind": "Vulcan 插件",
    "skills.plugins.tools": "工具",
    "skills.plugins.hooks": "Hook",
    "skills.plugins.sessions": "活动会话",
    "skills.plugins.diagnostics": "诊断",
    "skills.agent.emptyTitle": "技能目录尚未载入",
    "skills.agent.emptyDetail": "点击刷新或同步系统技能后会扫描系统目录与用户目录下的 SKILL.md。",
    "skills.agent.detailsTitle": "Agent Skill 详情",
    "skills.agent.detailsDescription": "查看 Agent Skill 详情。",
  },
  "en-US": {
    "app.subtitle": "Ai Coding",
    "notifications.appTitle": "Vulcan Code",
    "notifications.completed.generic": "Conversation completed",
    "notifications.completed.withTitle": "Conversation “{conversationTitle}” completed",
    "notifications.failed.generic": "Conversation failed",
    "notifications.failed.withTitle": "Conversation “{conversationTitle}” failed",
    "settings.notifications": "Notifications",
    "settings.notifications.description": "Manage conversation completion notifications, permission, and privacy for this client.",
    "settings.notifications.supported": "Supported by this shell",
    "settings.notifications.unsupported": "Unsupported by this shell",
    "settings.notifications.localChannel": "Local system notifications",
    "settings.notifications.localChannel.description": "Applies only to this browser or desktop client and is not synchronized to your account or other devices.",
    "settings.notifications.permission": "Permission",
    "settings.notifications.permission.unsupported": "Unsupported",
    "settings.notifications.permission.default": "Not authorized",
    "settings.notifications.permission.granted": "Granted",
    "settings.notifications.permission.denied": "Denied",
    "settings.notifications.requestPermission": "Request notification permission",
    "settings.notifications.requesting": "Requesting…",
    "settings.notifications.categories": "Notification categories",
    "settings.notifications.completed": "Conversation completed successfully",
    "settings.notifications.failed": "Conversation execution failed",
    "settings.notifications.privacy": "Notification privacy",
    "settings.notifications.privacy.description": "Notifications never include model response content. Conversation titles may appear on the lock screen.",
    "settings.notifications.showTitle": "Show conversation titles in notifications",
    "settings.notifications.limitations": "Capability boundaries",
    "settings.notifications.browserLimitation": "A closed browser page cannot receive local service events; background delivery requires a future Service Worker or remote endpoint.",
    "settings.notifications.desktopLimitation": "Notification identity, icons, and do-not-disturb behavior are controlled by the installed application and operating system.",
    "app.serviceConnectionRestored": "Local service connection restored.",
    "app.serviceReconnecting": "Local service connection lost. Reconnecting...",
    "app.serviceCheckingRecovery": "Local service connection lost. Checking recovery...",
    "area.chat": "Workspace",
    "area.skills": "Plugins",
    "area.agentSkills": "Skills",
    "area.agents": "Agents",
    "area.memory": "Memory",
    "area.knowledge": "Knowledge",
    "area.settings": "Settings",
    "area.chat.description": "Project sessions, message stream, and model input",
    "area.skills.description": "LuaSkills skills, plugins, and runtime settings",
    "area.agentSkills.description": "Standard Agent Skills catalog",
    "area.agents.description": "Agents, expert profiles, and delegation prompts",
    "area.memory.description": "VMM user, project, and conversation memory",
      "area.knowledge.description": "Project Markdown with Jieba BM25 search",
      "knowledge.name": "Knowledge library name",
      "knowledge.name.placeholder": "New knowledge library",
      "knowledge.create": "Create",
      "knowledge.filter.aria": "Filter knowledge libraries",
      "knowledge.filter.placeholder": "Filter libraries",
      "knowledge.refresh": "Refresh",
      "knowledge.emptyLibraries": "No knowledge libraries yet.",
      "knowledge.libraryCounts": "{documents} documents · {chunks} chunks · {status}",
      "knowledge.defaultDescription": "Markdown source with Jieba tokenization and BM25 search.",
      "knowledge.saveName": "Save name",
      "knowledge.view": "View",
      "knowledge.importDirectory": "Import directory",
      "knowledge.sources.title": "Import directories",
      "knowledge.sources.description": "Each directory is recorded independently. Synchronization hashes source files with SHA-256 and updates only added, changed, or deleted Markdown documents.",
      "knowledge.sources.add": "Add directory",
      "knowledge.sources.loading": "Loading import directories…",
      "knowledge.sources.empty": "No import directories have been added.",
      "knowledge.sources.documents": "{count} documents",
      "knowledge.sources.counts": "{documents} documents · {chunks} chunks",
      "knowledge.sources.lastScanned": "Last scanned {time}",
      "knowledge.sources.sync": "Incremental sync",
      "knowledge.sources.modify": "Change directory",
      "knowledge.sources.modifyPickerTitle": "Select a new Markdown directory for “{name}”",
      "knowledge.cancelImport": "Cancel import",
      "knowledge.cancelOperation": "Cancel operation",
      "knowledge.repairIndex": "Repair index",
      "knowledge.rebuildIndex": "Rebuild index",
      "knowledge.indexHealth.title": "Index integrity",
      "knowledge.indexHealth.description": "Checks authoritative chunks, physical FTS rows, content revisions, and Jieba dictionary revisions.",
      "knowledge.indexHealth.loading": "Checking index integrity…",
      "knowledge.indexHealth.searchable": "Searchable",
      "knowledge.indexHealth.needsRepair": "Repair required",
      "knowledge.indexHealth.authoritative": "Authoritative chunks",
      "knowledge.indexHealth.indexed": "Indexed rows",
      "knowledge.indexHealth.missing": "Missing or mismatched",
      "knowledge.indexHealth.orphaned": "Extra, orphaned, or mismatched",
      "knowledge.indexHealth.contentRevision": "Content revision {indexed} / {current}",
      "knowledge.indexHealth.dictionaryRevision": "Dictionary revision {indexed} / {current}",
      "knowledge.indexHealth.tokenizer": "Tokenizer {tokenizer}",
      "knowledge.delete": "Delete",
      "knowledge.deleteConfirm": "Permanently delete knowledge library “{name}”?",
      "knowledge.projectAuthorization": "Project authorization",
      "knowledge.documentProject": "Document project",
      "knowledge.sessionDefaultEnabled": "Enabled by default in sessions",
      "knowledge.currentSession": "Current session",
      "knowledge.selectionExplicit": "Explicit selection",
      "knowledge.selectionInherited": "Inheriting project defaults",
      "knowledge.documents": "Documents",
      "knowledge.search.placeholder": "Search content with Jieba BM25",
      "knowledge.search": "Search",
      "knowledge.searching": "Searching…",
      "knowledge.search.empty": "No matching content found.",
      "knowledge.viewer.maximize": "Maximize knowledge viewer",
      "knowledge.viewer.restore": "Restore knowledge viewer",
      "knowledge.viewer.loadingDocuments": "Loading library folders…",
      "knowledge.viewer.loadingFolder": "Loading…",
      "knowledge.viewer.loadMore": "Load more",
      "knowledge.viewer.loadingDocument": "Loading document…",
      "knowledge.viewer.selectDocument": "Select a document from the folder tree.",
      "knowledge.viewer.clearSearch": "Clear search",
      "knowledge.viewer.searchCount": "{count} documents",
      "knowledge.search.substring": "Substring match",
      "knowledge.body": "Body",
      "knowledge.documentCounts": "{lines} lines · {chunks} chunks",
      "knowledge.emptyDocuments": "This library has no browsable documents.",
      "knowledge.markdown.preview": "Markdown preview",
      "knowledge.markdown.source": "View source",
      "knowledge.previousPage": "Previous",
      "knowledge.page": "Page {page}",
      "knowledge.nextPage": "Next",
      "knowledge.close": "Close",
      "knowledge.emptySelection": "Create or select a knowledge library to begin.",
      "knowledge.importConfirm": "Import Markdown",
      "knowledge.pickerTitle": "Select a Markdown source directory",
      "knowledge.nameRequired": "Enter a knowledge library name.",
      "knowledge.operationCompleted": "{operation} completed.",
      "knowledge.importProgress": "{phase} · {files} files · {bytes} bytes",
      "knowledge.importCompleted": "Import completed: scanned {scanned}, changed {changed}, deleted {deleted}, chunks {chunks}.",
      "knowledge.importDialog.title": "Import Markdown knowledge",
      "knowledge.operationDialog.title.import": "Import Markdown knowledge",
      "knowledge.operationDialog.title.repair": "Repair knowledge index",
      "knowledge.operationDialog.title.rebuild": "Rebuild knowledge index",
      "knowledge.importDialog.preparing": "Creating background operation",
      "knowledge.importDialog.scanning": "Scanning: {discovered} Markdown files discovered",
      "knowledge.importDialog.processing": "Processing {processed} / {total} ({percent}%)",
      "knowledge.importDialog.completed": "Operation completed",
      "knowledge.importDialog.cancelled": "Operation cancelled",
      "knowledge.importDialog.failed": "Operation did not complete",
      "knowledge.importDialog.discovered": "{count} Markdown files discovered",
      "knowledge.importDialog.total": "{count} Markdown files total",
      "knowledge.importDialog.bytes": "{count} bytes read",
      "knowledge.importDialog.indexItems": "Index progress {processed} / {total}",
      "knowledge.importDialog.current": "Current directory, file, or phase",
      "knowledge.importDialog.waiting": "Waiting for operation progress…",
      "knowledge.importDialog.close": "Close",
      "knowledge.importDialog.cancelling": "Cancelling…",
      "knowledge.importDialog.cancellingDetail": "Cancellation requested; stopping after the current batch…",
      "knowledge.importDialog.committing": "Atomically committing knowledge content…",
      "knowledge.importDialog.indexing": "Updating search index {processed} / {total} ({percent}%)",
      "knowledge.importDialog.repairing": "Repairing search index {processed} / {total} ({percent}%)",
      "knowledge.importDialog.rebuilding": "Recovering search index {processed} / {total} ({percent}%)",
      "knowledge.importDialog.verifying": "Verifying the search index…",
      "knowledge.importDialog.finalizing": "Finalizing the import and clearing staging data…",
      "knowledge.importDialog.committingFallback": "Executing the atomic knowledge commit",
      "knowledge.importDialog.indexingFallback": "Updating the incremental search index",
      "knowledge.importDialog.repairingFallback": "Repairing missing or mismatched search rows",
      "knowledge.importDialog.rebuildingFallback": "Recovering search index from authoritative chunks",
      "knowledge.importDialog.verifyingFallback": "Checking index revisions and tokenizer",
      "knowledge.importDialog.finalizingFallback": "Updating library counts and clearing staging data",
      "knowledge.importPhase.scanning": "Scanning files",
      "knowledge.importPhase.parsing": "Parsing files",
      "knowledge.importPhase.committing": "Committing content",
      "knowledge.importPhase.indexing": "Updating index",
      "knowledge.importPhase.repairing": "Repairing index",
      "knowledge.importPhase.rebuilding": "Recovering index",
      "knowledge.importPhase.verifying": "Verifying index",
      "knowledge.importPhase.finalizing": "Finalizing import",
      "knowledge.importPhase.completed": "Completed",
      "knowledge.importPhase.cancelled": "Cancelled",
      "knowledge.composer.aria": "Select knowledge libraries for this session",
      "knowledge.composer.label": "Knowledge",
      "knowledge.composer.manage": "Manage knowledge libraries",
      "knowledge.composer.noneAuthorized": "No knowledge libraries are authorized for this project",
      "knowledge.package.import": "Import library",
      "knowledge.package.export": "Export library",
      "knowledge.package.exportPickerTitle": "Select a package export directory",
      "knowledge.package.importPickerTitle": "Select a .vkb knowledge package",
      "knowledge.package.noPackages": "This directory contains no child directories or .vkb packages.",
      "knowledge.package.inspecting": "Inspecting…",
      "knowledge.package.inspect": "Inspect and continue",
      "knowledge.package.importTitle": "Import complete knowledge library",
      "knowledge.package.counts": "{documents} documents · {chunks} chunks · {bytes}",
      "knowledge.package.nameConflict": "A library named “{name}” already exists. It will not be overwritten by default; rename the import or explicitly choose replacement.",
      "knowledge.package.createStrategy": "Create new library",
      "knowledge.package.renameRequired": "Enter an unused name. The existing library will remain unchanged.",
      "knowledge.package.createDescription": "Create a new library with the package name and preserve every existing library.",
      "knowledge.package.replaceStrategy": "Replace existing library",
      "knowledge.package.replaceDescription": "Preserve the target ID, project access, and session references while replacing all library content.",
      "knowledge.package.replaceConfirm": "Replace “{name}”? Its {documents} documents and {chunks} chunks will be replaced by the package. The library ID, project access, and session references will be preserved.",
      "knowledge.package.exportCompleted": "Knowledge package exported to: {path}",
      "knowledge.package.importCompleted": "Complete knowledge library imported without rebuilding its index.",
      "knowledge.package.replaceCompleted": "Knowledge library safely replaced; project access and session references were preserved.",
    "area.settings.description": "Projects, models, tools, diagnostics, and system settings",
    "feature.status.available": "Available",
    "feature.status.preview": "Preview",
    "global.openSettings": "Open settings",
    "global.toggleDark": "Switch to dark theme",
    "global.toggleLight": "Switch to light theme",
    "global.language": "Language",
    "global.languageSaveFailed": "Failed to save language: {error}",
    "global.online": "Online",
    "global.preview": "Preview",
    "global.progress": "Progress",
    "global.labelSeparator": ": ",
    "global.listDelimiter": ", ",
    "appChrome.menu": "Open app menu",
    "appChrome.status": "Status",
    "appChrome.theme": "Theme",
    "appChrome.theme.dark": "Dark",
    "appChrome.theme.light": "Light",
    "appChrome.language.zh": "Chinese",
    "appChrome.scrollFeaturesLeft": "Scroll main features left",
    "appChrome.scrollFeaturesRight": "Scroll main features right",
    "appChrome.windowActionFailed": "Window action failed: ",
    "appChrome.window.minimize": "Minimize window",
    "appChrome.window.restore": "Restore window",
    "appChrome.window.maximize": "Maximize window",
    "appChrome.window.close": "Close window",
    "appChrome.githubOpenFailed": "Failed to open GitHub: ",
    "appChrome.githubAria": "Open the OpenVulcan/vulcan-code GitHub repository",
    "sidebar.user.local": "Local user",
    "sidebar.user.account": "Account",
    "sidebar.user.logout": "Log out",
    "projectMenu.conversationsAria": "{project} conversations",
    "projectMenu.newConversation": "New conversation",
    "projectMenu.noConversations": "No conversations",
    "projectMenu.pin": "Pin project",
    "projectMenu.unpin": "Unpin project",
    "projectMenu.openDirectory": "Open in file explorer",
    "projectMenu.createWorktree": "Create permanent worktree",
    "projectMenu.renameProject": "Rename project",
    "projectMenu.setIcon": "Set project icon",
    "projectMenu.archiveConversations": "Archive conversations",
    "projectMenu.removeProject": "Remove project",
    "projectMenu.projectSettings": "Project settings",
    "conversationMenu.pin": "Pin conversation",
    "conversationMenu.unpin": "Unpin conversation",
    "conversationMenu.rename": "Rename conversation",
    "conversationMenu.archive": "Archive conversation",
    "conversationMenu.markUnread": "Mark as unread",
    "conversationMenu.openDirectory": "Open in file explorer",
    "conversationMenu.copyTitle": "Copy title",
    "conversationMenu.copyWorkspace": "Copy workspace path",
    "conversationMenu.copySessionId": "Copy session ID",
    "conversationMenu.copyDeepLink": "Copy deep link",
    "conversationMenu.forkLocal": "Fork locally",
    "conversationMenu.forkWorktree": "Fork to new worktree",
    "conversationMenu.openNewWindow": "Open in new window",
    "conversationMenu.moreTitleActions": "More title actions",
    "diffAction.moreStaging": "More staging actions",
    "diffAction.unstageAll": "Unstage all",
    "diffAction.unstageFile": "Unstage current file",
    "diffAction.restoreAll": "Restore all",
    "diffAction.stageAll": "Stage all",
    "diffAction.stageFile": "Stage this file",
    "diffAction.restoreFile": "Restore this file",
    "diff.mode.unstaged": "Unstaged",
    "diff.mode.staged": "Staged",
    "diff.mode.commit": "Unpushed commits",
    "diff.mode.branch": "Branch",
    "diff.branch.current": "Current branch",
    "diff.branch.main": "Main branch",
    "diff.view.toUnified": "Switch to unified view",
    "diff.view.toSplit": "Switch to split view",
    "diff.refresh": "Refresh diff",
    "diff.loading": "Loading",
    "diff.moreActions": "More diff actions",
    "diff.copyRaw": "Copy raw diff",
    "diff.inline.disable": "Disable inline differences",
    "diff.inline.enable": "Enable inline differences",
    "diff.wrap.disable": "Disable wrapping",
    "diff.wrap.enable": "Enable wrapping",
    "diff.contextControls.hide": "Hide context expansion controls",
    "diff.contextControls.show": "Show context expansion controls",
    "diff.indicator.useClassic": "Use +/- row markers",
    "diff.indicator.useBars": "Use colored row bars",
    "diff.search.open": "Open diff search",
    "diff.search.label": "Search diff",
    "diff.search.truncated": "Results truncated",
    "diff.search.previous": "Previous result",
    "diff.search.next": "Next result",
    "diff.search.indexUnavailable": "Index unavailable",
    "diff.commit.none": "No unpushed commits",
    "diff.branch.search": "Search branches",
    "diff.branch.label": "Branch",
    "diff.branch.noMatches": "No matching branches",
    "diff.emptyBody": "No Git Diff is available.",
    "diff.noMatchingFiles": "No matching files",
    "diff.file.tooLarge": "Oversized file not shown",
    "diff.file.added": "Added file",
    "diff.file.deleted": "Deleted file",
    "diff.file.gitlink": "Submodule pointer changed. Text content was not rendered.",
    "diff.file.renamedNoChanges": "File renamed without changes",
    "diff.file.noContent": "No content",
    "diff.render.changedLinesSkipped": "Changed lines exceed {limit}; highlight details were skipped by the lightweight policy",
    "diff.render.skipped": "Highlight details were skipped by the lightweight policy",
    "diff.render.rowsLightweight": "{count} rows rendered in lightweight mode",
    "diff.large.totalFiles": "File count exceeds {limit}",
    "diff.large.totalChangedLines": "Total changed lines exceed {limit}",
    "diff.large.totalChangedBytes": "Total changed size exceeds {limit}",
    "diff.large.fileChangedLines": "File changed lines exceed {limit}",
    "diff.large.fileChangedBytes": "File changed size exceeds {limit}",
    "diff.large.fileMaxLineBytes": "Single-line size exceeds {limit}",
    "diff.load.rowsPending": "{rows} rows pending, about {hunks} hunks",
    "diff.load.retry": "Retry",
    "diff.load.reveal": "Open location",
    "diff.load.matchCount": "{count} matches",
    "diff.load.backendMatchFallback": "The full backend diff matched this file.",
    "diff.load.loadingDetail": "Loading file diff details.",
    "diff.load.timeout": "File diff detail loading timed out.",
    "diff.load.failed": "File diff detail loading failed.",
    "diff.load.loadedEmpty": "File diff details loaded, but no text content can be shown.",
    "diff.load.summaryIdle": "This file is in summary state. Details will load when it enters the viewport.",
    "console.title": "Conversation",
    "console.newConversation": "New conversation",
    "console.emptyPrompt": "What should we build in {projectName}?",
    "conversation.loading.title": "Loading conversation",
    "conversation.loading.requesting": "Requesting conversation data",
    "conversation.loading.locating": "Locating conversation",
    "conversation.loading.index": "Reading message index",
    "conversation.loading.reading": "Reading message content",
    "conversation.loading.subagents": "Restoring open agent index",
    "conversation.loading.projecting": "Preparing display data",
    "conversation.loading.done": "Conversation loaded",
    "conversation.loading.failed": "Conversation body failed to load",
    "conversation.unlinkedProject": "Unlinked project",
    "conversation.history.loadMore": "Load more history",
    "conversation.history.loading": "Loading history",
    "conversation.action.copyFinal": "Copy final output source",
    "conversation.action.copiedFinal": "Copied final output source",
    "conversation.action.fork": "Fork conversation",
    "conversation.action.forkTitleSuffix": " (fork)",
    "conversation.action.forkFailed": "Fork failed",
    "conversation.action.forkSucceeded": "Forked conversation created",
    "conversation.action.forkInProgressTitle": "Forking conversation",
    "conversation.action.forkInProgressMessage": "Copying the conversation and opening the new fork. Please wait.",
    "conversation.forkSource.label": "Derived from conversation",
    "conversation.forkSource.missing": "The original conversation no longer exists",
    "conversation.block.toolCall": "Call tool",
    "conversation.block.toolResult": "Tool result",
    "conversation.block.command": "Command session",
    "conversation.block.thinking": "Thinking process",
    "conversation.block.plan": "Plan",
    "conversation.block.notice": "Run notice",
    "conversation.block.detail": "Detail",
    "conversation.metadata.type": "Type",
    "conversation.metadata.status": "Status",
    "conversation.metadata.tool": "Tool",
    "conversation.metadata.identifier": "Identifier",
    "conversation.metadata.error": "Error",
    "conversation.metadata.result": "Result",
    "conversation.metadata.returned": "Returned",
    "conversation.toolRequest.receiving": "Receiving data",
    "conversation.toolRequest.ready": "Data received",
    "conversation.toolRequest.characters": "characters",
    "conversation.stats.providerCalls": "{count} calls",
    "conversation.stats.toolCalls": "{count} tools",
    "conversation.stats.totalTokens": "{count} tokens",
    "conversation.stats.cachedTokens": "Cache hit {count}",
    "conversation.stats.cachedTokensCompact": "Cache {count} tokens",
    "conversation.stats.outputSpeed": "{speed}/s",
    "conversation.stats.group.regular": "Regular {count} tokens",
    "conversation.stats.group.subagent": "Agent {count} tokens",
    "conversation.stats.group.contextCompaction": "Compaction {count} tokens",
    "conversation.subagent.stopTask": "Stop",
    "conversation.stopWithAgents.title": "Stop the current conversation?",
    "conversation.stopWithAgents.description": "{count} agent(s) are still working. Choose whether to stop only the main conversation or stop those agents too.",
    "conversation.stopWithAgents.mainOnly": "Stop main only",
    "conversation.stopWithAgents.all": "Stop agents too",
    "conversation.process.header": "Process",
    "conversation.process.stoppedAfter": "You stopped after {duration}",
    "conversation.process.stopped": "You stopped this turn",
    "conversation.process.duration": "Duration {duration}",
    "conversation.toolSequence.summary": "Ran {count} tool calls",
    "conversation.toolSequence.runningSummary": "Ran {completed} calls, running {running} tools",
    "conversation.action.editPrompt": "Edit user input",
    "conversation.action.editPromptFailed": "Edit user input failed",
    "conversation.action.cancel": "Cancel",
    "conversation.action.saveAndReplay": "Save and replay",
    "contextCompaction.title.running": "Compacting context",
    "contextCompaction.title.completed": "Context compacted",
    "contextCompaction.title.failed": "Context compaction failed",
    "contextCompaction.generating": "Generating the context compaction summary.",
    "contextCompaction.thinkingLabel": "Thinking",
    "contextCompaction.thinkingActive": "Thinking",
    "contextCompaction.emptyThinking": "No thinking content yet",
    "contextCompaction.failed.trigger": "Trigger",
    "contextCompaction.failed.error": "Error",
    "contextCompaction.failed.note": "API compaction did not complete, so the context anchor was not changed. Future requests will keep using the original context window.",
    "contextCompaction.trigger.manual": "Manual",
    "contextCompaction.trigger.auto": "Automatic",
    "contextCompaction.progress.prepareInput": "Preparing compaction input.",
    "contextCompaction.progress.buildRequest": "Compaction input is ready; building the model summary request.",
    "contextCompaction.progress.prepareRuntime": "Preparing the isolated compaction runtime and tool snapshot.",
    "contextCompaction.progress.runtimeReady": "The compaction runtime is ready; starting the provider request.",
    "contextCompaction.progress.prepared": "The model request is prepared.",
    "contextCompaction.progress.requestReady": "The model request snapshot is built; converting it to the provider payload.",
    "contextCompaction.progress.providerCheckpoint": "Recorded the provider request checkpoint.",
    "contextCompaction.progress.providerLoweringRepair": "Applied provider request compatibility repair.",
    "contextCompaction.progress.wireRequestReady": "The provider request payload is ready to send.",
    "contextCompaction.progress.requestStarted": "The provider request has been sent; waiting for the first output.",
    "contextCompaction.progress.thinkingDelta": "The model has started streaming thinking content.",
    "contextCompaction.progress.outputDelta": "The model has started generating the compaction summary.",
    "contextCompaction.progress.prepareExecution": "Preparing the compaction execution environment and tool snapshot.",
    "contextCompaction.progress.executionReady": "The compaction execution environment is ready; starting the runtime.",
    "contextCompaction.progress.bootstrapRuntime": "Starting the isolated compaction runtime.",
    "contextCompaction.progress.unknown": "Context compaction is running.",
    "contextPanel.open": "Open right panel",
    "contextPanel.close": "Close right panel",
    "contextPanel.review": "Review",
    "contextPanel.memory": "Memory",
    "contextPanel.files": "Files",
    "contextPanel.dynamic": "Dynamic",
    "contextPanel.static": "Static",
    "contextPanel.reviewEmpty": "Git Diff review panel is pending",
    "contextPanel.memoryEmpty": "Memory injection panel is pending",
    "contextPanel.dynamicEmpty": "This conversation has no active dynamic TTL injection.",
    "contextPanel.staticEmpty": "This conversation has no active static context.",
    "contextUsage.title": "Context usage",
    "contextUsage.waiting": "Request split: waiting for budget engine",
    "sidebar.console": "Workspace",
    "sidebar.pinned": "Pinned",
    "sidebar.projects": "Projects",
    "sidebar.unlinkedConversations": "Unlinked conversations",
    "sidebar.addProject": "Add project",
    "sidebar.noProjects": "No projects",
    "sidebar.waitingWorkspace": "Waiting for Workspace snapshot",
    "sidebar.overview": "Overview",
    "sidebar.settingsCategories": "Settings",
    "sidebar.toggle": "Toggle sidebar",
    "composer.mode.execute": "Build",
    "composer.mode.plan": "Plan",
    "composer.capability.extended": "Extended Capability",
    "composer.capability.extended.subtitle": "Capability settings",
    "composer.capability.basic": "Basic Capability",
    "composer.capability.basic.subtitle": "Text and code only",
    "composer.capability.enabledCount": "{enabled}/{total} enabled",
    "composer.capability.tooltipTitle": "Configure extended capabilities",
    "composer.capability.tooltipEmpty": "No enabled modules",
    "composer.capability.dialogTitle": "Current session capabilities",
    "composer.capability.dialogDescription": "Changes are saved to this session only after you click Save, and affect the next turn's cache hit rate.",
    "composer.capability.source.nativeWithTargets": "Native + {count} targets",
    "composer.capability.source.native": "Native model",
    "composer.capability.source.targets": "{count} targets",
    "composer.capability.source.noTargets": "No targets",
    "composer.capability.empty.native": "This capability is handled natively by the selected model.",
    "composer.capability.empty.noTargets": "This capability has no configured targets.",
    "composer.capability.saveHint.externalUpdate": "This session was updated elsewhere; the current draft is not synced.",
    "composer.capability.saveHint.saving": "Saving to this session...",
    "composer.capability.saveHint.unsaved": "Unsaved changes.",
    "composer.capability.saveHint.clean": "No unsaved changes.",
    "composer.capability.useDefaults": "Use defaults",
    "composer.capability.save.saving": "Saving",
    "composer.capability.save.saved": "Saved",
    "composer.capability.save": "Save",
    "composer.capability.close.title": "Save changes?",
    "composer.capability.close.description": "This session capability dialog has unsaved changes.",
    "composer.capability.close.discard": "Discard",
    "composer.capability.close.keepEditing": "Keep editing",
    "composer.reasoning.off": "Off",
    "composer.reasoning.low": "Low",
    "composer.reasoning.medium": "Medium",
    "composer.reasoning.high": "High",
    "composer.reasoning.max": "Max",
    "composer.reasoning.subtitle": "Reasoning level",
    "composer.placeholder": "Enter task instruction for this conversation ({shortcut})",
    "composer.shortcutHint": "⏎ Send, Shift + ⏎ New line",
    "composer.selectFallback": "Select",
    "composer.aria.input": "Current conversation input",
    "composer.aria.mode": "Select build mode",
    "composer.aria.model": "Select model",
    "composer.aria.capability": "Select extended capability",
    "composer.aria.reasoning": "Select reasoning level",
    "composer.aria.send": "Send",
    "composer.aria.stop": "Stop",
    "composer.modelPicker.filter": "Filter by model name, id, or provider",
    "composer.modelPicker.noMatches": "No models match the current filter.",
    "composer.attachment.add": "Add attachment",
    "composer.attachment.extension": "Extension",
    "composer.attachment.menu": "Attachments",
    "composer.attachment.empty": "No attachment types are available",
    "composer.attachment.remove": "Remove attachment",
    "composer.attachment.unsupported": "These attachments are not supported by the current model or extended capabilities.",
    "composer.attachment.maxLimit": "You can add at most {count} attachments.",
    "composer.attachment.partialMaxLimit": "The {count}-attachment limit has been reached. Some files were not added.",
    "composer.attachment.kind.image": "Image",
    "composer.attachment.kind.audio": "Audio",
    "composer.attachment.kind.video": "Video",
    "composer.attachment.kind.pdf": "PDF",
    "composer.thinkingProcess": "Thinking",
    "composer.planMode": "Plan mode",
    "composer.vmmMemory": "VMM memory",
    "composer.vmm.precheckInjection": "Precheck memory injection",
    "composer.vmm.memoryWriteback": "Post-turn memory writeback",
    "composer.vmm.mode.inheritProject": "Follow project",
    "composer.vmm.mode.enabled": "On",
    "composer.vmm.mode.disabled": "Off",
    "composer.goal": "Goal",
    "composer.scrollToBottom": "Scroll to bottom",
    "composer.goalDialog.resumeTitle": "Resume task",
    "composer.goalDialog.editTitle": "Edit goal",
    "composer.goalDialog.objectiveAria": "Goal content",
    "composer.goalDialog.objectivePlaceholder": "Enter the goal this conversation should keep pursuing",
    "composer.goalDialog.tokenBudgetAria": "Token budget",
    "composer.goalDialog.tokenBudgetPlaceholder": "Token budget (optional)",
    "composer.goalDialog.save": "Save",
    "composer.goal.status.active": "Active goal",
    "composer.goal.status.paused": "Paused goal",
    "composer.goal.status.blocked": "Blocked goal",
    "composer.goal.status.usageLimited": "Usage-limited goal",
    "composer.goal.status.budgetLimited": "Budget exhausted",
    "composer.goal.status.completed": "Completed goal",
    "composer.goal.tooltip.input": "Input (new)",
    "composer.goal.tooltip.cache": "Input (cached)",
    "composer.goal.tooltip.output": "Output",
    "composer.goal.tooltip.reasoning": "Reasoning",
    "composer.goal.tooltip.total": "Total",
    "composer.goal.action.edit": "Edit goal",
    "composer.goal.action.resumeTask": "Resume task",
    "composer.goal.action.resume": "Resume goal",
    "composer.goal.action.pause": "Pause goal",
    "composer.goal.action.clear": "Clear goal",
    "composer.goal.error.empty": "Goal content cannot be empty.",
    "composer.goal.error.budgetInvalid": "Token budget must be a positive integer.",
    "composer.planMode.label": "Plan",
    "composer.planMode.goalConflict": "Plan mode and Goal mode cannot both be enabled.",
    "composer.error.stopCurrentFailed": "Failed to stop the current conversation.",
    "composer.error.promptEmpty": "Enter a task instruction.",
    "composer.modelPicker.tooltipTitle": "Select model",
    "composer.modelPicker.currentModel": "Current model: {model}",
    "composer.modelPicker.currentProvider": "Current provider: {provider}",
    "composer.secondaryRoute.select": "Select secondary provider",
    "composer.secondaryRoute.search": "Search secondary providers",
    "composer.secondaryRoute.autoEndpoint": "OpenRouter automatically selects the endpoint",
    "composer.secondaryRoute.loading": "Loading secondary providers...",
    "composer.secondaryRoute.noMatches": "No secondary providers match",
    "composer.secondaryRoute.switchFailed": "Secondary provider switch failed. The previous selection was kept.",
    "composer.secondaryRoute.switched": "Secondary provider switched.",
    "composer.secondaryRoute.restoredAuto": "Secondary provider restored to Auto.",
    "composer.progress.open": "View current progress",
    "composer.progress.progressLabel": "Progress {current}/{total}",
    "composer.progress.step": "Step {current} / {total}",
    "composer.progress.changedFiles": "{count} changed files",
    "composer.progress.stepInProgress": "Step in progress",
    "composer.reasoning.tooltip": "Reasoning level",
    "pendingPrompt.queue": "Queued prompts",
    "pendingPrompt.drag": "Drag queued prompt",
    "pendingPrompt.delete": "Delete queued prompt",
    "pendingPrompt.edit": "Edit queued prompt",
    "pendingPrompt.insert": "Guide queued prompt",
    "pendingPrompt.inserted": "Queued prompt guided",
    "pendingPrompt.insertShort": "Guide",
    "pendingPrompt.insertedShort": "Guided",
    "pendingPrompt.attachmentCount": "{count} attachments",
    "pendingPrompt.attachmentMessage": "Attachment message",
    "sessionActions.noProjectForCapability": "The current conversation is not linked to a project, so AI capability settings cannot be saved.",
    "sessionActions.noConversationForCapability": "Create or select a conversation before saving AI capability settings.",
    "sessionActions.previewCannotSaveCapability": "Session AI capability settings cannot be saved in browser preview mode.",
    "sessionActions.capabilitySaved": "Saved AI capability settings for the current session.",
    "sessionActions.noConversationForGoalSave": "Create or select a conversation before setting a goal.",
    "sessionActions.previewCannotSaveGoal": "Goals cannot be saved in browser preview mode.",
    "sessionActions.goalSaved": "Goal saved.",
    "sessionActions.noExistingConversation": "Select an existing conversation first.",
    "sessionActions.previewCannotResumeGoal": "Goals cannot be resumed in browser preview mode.",
    "sessionActions.goalResumed": "Goal resumed.",
    "sessionActions.previewCannotClearGoal": "Goals cannot be cleared in browser preview mode.",
    "sessionActions.goalCleared": "Goal cleared.",
    "sessionActions.previewCannotUpdateGoal": "Goal status cannot be updated in browser preview mode.",
    "sessionActions.noConversationForPlanMode": "Create or select a conversation before toggling Plan mode.",
    "sessionActions.previewCannotTogglePlanMode": "Plan mode cannot be toggled in browser preview mode.",
    "sessionActions.planModeEnabled": "Plan mode enabled.",
    "sessionActions.planModeDisabled": "Plan mode disabled.",
    "sessionActions.noConversationForVmmPolicy": "Create or select a conversation before changing VMM memory policy.",
    "sessionActions.previewCannotSetVmmPolicy": "VMM memory policy cannot be changed in browser preview mode.",
    "sessionActions.vmmPolicySaved": "VMM memory policy saved.",
    "conversationActions.noProjectForSend": "The current conversation is not linked to a project, so it cannot be sent.",
    "conversationActions.previewCannotSend": "Real conversations cannot be sent in browser preview mode.",
    "conversationActions.previewCannotCompact": "Real context compaction cannot run in browser preview mode.",
    "conversationActions.modelSwitched": "Current model switched to {model} ({provider}).",
    "projectActions.vmmSaved": "Project VMM settings saved.",
    "projectActions.openRequested": "Requested to open project directory: {project}",
    "projectActions.unpinned": "Unpinned project: {project}",
    "projectActions.pinned": "Pinned project: {project}",
    "projectActions.iconUpdated": "Updated project icon: {project}",
    "projectActions.iconReset": "Reset project icon: {project}",
    "projectActions.listUpdated": "Project list updated.",
    "projectActions.skillInherited": "Set {skill} to inherit global settings",
    "projectActions.skillEnabled": "Enabled {skill} for this project",
    "projectActions.skillDisabled": "Disabled {skill} for this project",
    "projectSettings.cleanupCompleted": "Project attachment cache cleanup completed.",
    "projectDialog.renameProjectTitle": "Rename project",
    "projectDialog.renameConversationTitle": "Rename conversation",
    "projectDialog.renameProjectDescription": "Enter a new project display name. The project path will not change; confirming an empty name restores the folder name.",
    "projectDialog.renameConversationDescription": "Enter a new conversation name.",
    "projectDialog.renamePlaceholder": "Enter a new name",
    "projectDialog.confirm": "Confirm",
    "projectDialog.archiveRemoveTitle": "Archive and remove project?",
    "projectDialog.archiveTitle": "Archive {count} conversations?",
    "projectDialog.archiveRemoveDescription": "This archives {count} conversations in {project}, then removes the project from the project list. Archived conversations appear in archive restore after you add this project again.",
    "projectDialog.archiveDescription": "This archives conversations in {project}. The project remains in the left project list.",
    "projectDialog.currentProject": "current project",
    "projectDialog.archiveAndRemove": "Archive and remove",
    "projectDialog.archiveAll": "Archive all",
    "projectDialog.removing": "Removing",
    "projectDialog.archiving": "Archiving",
    "projectRow.conversationMenuAria": "{project} conversation menu",
    "projectRow.createConversationAria": "Create a new conversation in {project}",
    "projectRow.collapseConversations": "Collapse",
    "projectRow.showMoreConversations": "Show more",
    "projectRow.noConversations": "No conversations",
    "conversationRow.running": "Conversation is running",
    "conversationRow.unread": "Unread",
    "conversationRow.askAttention": "Question waiting for reply",
    "conversationRow.planAttention": "Plan confirmation waiting",
    "conversationRow.confirmArchive": "Confirm archive conversation",
    "projectFolderPicker.title": "Choose project directory",
      "projectFolderPicker.pathAria": "Directory path",
      "projectFolderPicker.openPath": "Open the entered path",
    "projectFolderPicker.parent": "Parent",
    "projectFolderPicker.rootsAria": "Drives",
    "projectFolderPicker.noRoots": "No drives available",
    "projectFolderPicker.hidden": "Hidden",
    "projectFolderPicker.noChildren": "No enterable subdirectories",
    "projectFolderPicker.adding": "Adding",
    "projectIcon.title": "Set project icon",
    "projectIcon.description": "Choose a fixed icon for {project}, or generate a letter badge. Resetting restores the default folder toggle icon.",
    "projectIcon.previewAria": "Project icon preview",
    "projectIcon.customTab": "Custom",
    "projectIcon.filterPlaceholder": "Filter {pack} icons",
    "projectIcon.loadMore": "Load more",
    "projectIcon.text": "Text",
    "projectIcon.textPlaceholder": "1 Chinese character or 2 English letters",
    "projectIcon.shape": "Shape",
    "projectIcon.shapeCircle": "Circle",
    "projectIcon.shapeSquare": "Square",
    "projectIcon.backgroundColor": "Background",
    "projectIcon.borderColor": "Border",
    "projectIcon.foregroundColor": "Text color",
    "projectIcon.resetDefault": "Reset default",
    "projectIcon.set": "Set",
    "projectSettings.dialogTitle": "Project settings: {project}",
    "projectSettings.dialogDescription": "Project-level configuration and cache cleanup only affect the current project space.",
    "projectSettings.categoryAria": "Project settings categories",
    "projectSettings.overview": "Overview",
    "projectSettings.pinnedProject": "Pinned project",
    "projectSettings.projectSpaceSummary": "Project space summary",
    "projectSettings.vmmMemory": "VMM memory",
    "projectSettings.knowledgeBases": "Knowledge bases",
    "projectSettings.knowledgeBasesDetail": "Project access and session inheritance defaults",
    "projectSettings.knowledgeBasesDescription": "Authorization controls which libraries this project may use. Default enablement controls which libraries new sessions inherit.",
    "projectSettings.knowledgeBasesLoading": "Loading knowledge-base access…",
    "projectSettings.knowledgeBasesSaved": "Knowledge-base access saved.",
    "projectSettings.projectOverride": "Project override",
    "projectSettings.inheritGlobal": "Inherit global",
    "projectSettings.promptSkillCount": "{count} skills",
    "projectSettings.attachmentCache": "Attachment cache",
    "projectSettings.attachmentCleanup": "Historical attachment cleanup",
    "projectSettings.projectSpace": "Project space",
    "projectSettings.projectId": "Project ID: {id}",
    "projectSettings.conversationCount": "Conversation count: {count}",
    "projectSettings.pinStatus": "Pinned status: {status}",
    "projectSettings.pinned": "Pinned",
    "projectSettings.notPinned": "Not pinned",
    "projectSettings.vmmHasOverride": "This project has an independent override enabled.",
    "projectSettings.vmmInheritsGlobal": "This project inherits global VMM settings.",
    "projectSettings.agentOverrideCount": "This project has {count} overrides.",
    "projectSettings.agentInheritsGlobal": "No project-level override is set; global rules are inherited.",
    "projectSettings.vmmTitle": "Project VMM memory",
    "projectSettings.codebaseMemory.title": "Codebase Memory",
    "projectSettings.codebaseMemory.description": "Enable host-managed multi-project code graph, search, and change awareness for this project.",
    "projectSettings.codebaseMemory.enabled": "Managed service enabled",
    "projectSettings.codebaseMemory.disabled": "Disabled by default",
    "projectSettings.codebaseMemory.loading": "Loading Codebase Memory status...",
    "projectSettings.codebaseMemory.enable": "Enable Codebase Memory for this project",
    "projectSettings.codebaseMemory.enableHint": "The project joins the shared managed service; sessions receive read-only tools scoped to this project.",
    "projectSettings.codebaseMemory.disableHint": "AI tools are not registered and existing indexes are retained.",
    "projectSettings.codebaseMemory.serviceStatus": "Managed service status",
    "projectSettings.codebaseMemory.indexStatus": "Project index status",
    "projectSettings.codebaseMemory.lastIndexed": "Last successful index",
    "projectSettings.codebaseMemory.revision": "Index revision",
    "projectSettings.codebaseMemory.unavailable": "Unavailable",
    "projectSettings.codebaseMemory.reindex": "Reindex project",
    "projectSettings.codebaseMemory.enabledSaved": "Project Codebase Memory enabled.",
    "projectSettings.codebaseMemory.disabledSaved": "Project Codebase Memory disabled; existing indexes were retained.",
    "projectSettings.codebaseMemory.reindexAccepted": "Project reindex request submitted.",
    "projectSettings.vmmDescription": "Override memory enablement, user/project mapping, and recall injection parameters for this project.",
    "projectSettings.vmmLoading": "Loading project VMM settings...",
    "projectSettings.enableProjectOverride": "Enable project override",
    "projectSettings.overrideEnabledHint": "After saving, this project uses the settings below.",
    "projectSettings.overrideDisabledHint": "After saving, project overrides are cleared and global settings are inherited.",
    "projectSettings.vmmTransportHint": "Project-level settings inherit the global gRPC connection and API Key; adjust connection parameters in global VMM memory settings.",
    "projectSettings.saveVmm": "Save VMM settings",
    "projectSettings.agentTitle": "Project Agent Skills",
    "projectSettings.agentDescription": "Create project-specific enablement overrides for system and user skills.",
    "projectSettings.agent.globalEnabled": "Global enabled",
    "projectSettings.agent.globalDisabled": "Global disabled",
    "projectSettings.agentLoading": "Loading project skill overrides...",
    "projectSettings.agentEmpty": "No skills were found in system or user directories.",
    "projectSettings.agent.projectEnabled": "Project enabled",
    "projectSettings.agent.projectDisabled": "Project disabled",
    "projectSettings.agent.inheritEnabled": "Inherit enabled",
    "projectSettings.agent.inheritDisabled": "Inherit disabled",
    "projectSettings.agent.optionInheritEnabled": "Inherit global (enabled)",
    "projectSettings.agent.optionInheritDisabled": "Inherit global (disabled)",
    "projectSettings.agent.projectStateAria": "{skill} project state",
    "projectSettings.cleanupTitle": "Project attachment cache cleanup",
    "projectSettings.cleanupDescription": "Filter this project's managed historical attachments by date and attachment type.",
    "projectSettings.managedDirectory": "Managed directory",
    "projectSettings.cutoffDate": "Cutoff date",
    "projectSettings.attachmentKind.image": "Image",
    "projectSettings.attachmentKind.audio": "Audio",
    "projectSettings.attachmentKind.video": "Video",
    "projectSettings.cleanup.previewing": "Previewing",
    "projectSettings.cleanup.preview": "Preview cleanup scope",
    "projectSettings.cleanup.cleaning": "Cleaning",
    "projectSettings.cleanup.execute": "Run cleanup",
    "projectSettings.lastPreview": "Latest preview",
    "projectSettings.lastExecution": "Latest execution",
    "projectSettings.cleanup.notExecuted": "Historical attachment cache cleanup has not run yet.",
    "projectSettings.cleanup.previewFallback": "Preview mode will inspect managed {kinds} attachments before {date}.",
    "projectSettings.cleanup.invalidDate": "Choose a valid cutoff date.",
    "projectSettings.cleanup.selectKind": "Select at least one attachment type before previewing.",
    "projectSettings.cleanup.initialNotice": "Choose a cutoff date and attachment types to preview the managed attachment cleanup scope for this project.",
    "shell.windowOperationFailed": "Window operation failed: {error}",
    "shell.openGithubFailed": "Failed to open GitHub: {error}",
    "shell.minimizeWindow": "Minimize window",
    "shell.maximizeWindow": "Maximize window",
    "shell.restoreWindow": "Restore window",
    "shell.closeWindow": "Close window",
    "shell.openRepository": "Open the OpenVulcan/vulcan-code GitHub repository",
    "startup.loadingProjectIndex": "Loading project and conversation indexes",
    "startup.connectingService": "Connecting to Vulcan Core Service",
    "startup.loadingStep": "Loading {step}",
    "startup.loadingWorkspace": "Loading Workspace",
    "startup.downloadProgress": "Download progress",
    "startup.failed": "Startup failed: {error}",
    "workspaceFiles.copyContentSuccess": "Copied file content.",
    "workspaceFiles.noProjectDirectory": "The current conversation is not linked to a project directory.",
    "workspaceFiles.noMatchedFiles": "No matching files.",
    "workspaceFiles.noDisplayableFiles": "No displayable files.",
    "workspaceFiles.filterPlaceholder": "Filter files...",
    "workspaceFiles.loading": "Loading.",
    "workspaceFiles.loadingTree": "Loading file tree.",
    "workspaceFiles.filterRules": "File filter rules",
    "workspaceFiles.hiddenRules": "Hidden rules",
    "workspaceFiles.ignoreHiddenFiles": "Ignore hidden files",
    "workspaceFiles.ignoreDotNames": "Ignore dot-prefixed names",
    "workspaceFiles.useIgnoreRules": "Use .ignore rules",
    "workspaceFiles.useGitignoreRules": "Use .gitignore rules",
    "workspaceFiles.filterNonPreviewFiles": "Filter non-code, image, and video files",
    "workspaceFiles.fileActions": "File actions",
    "workspaceFiles.copyPath": "Copy path",
    "workspaceFiles.copyRelativePath": "Copy relative path",
    "workspaceFiles.copyFileContent": "Copy file content",
    "workspaceFiles.filterFiles": "Filter files",
    "workspaceFilePreview.openFile": "Open file",
    "workspaceFilePreview.emptyHint": "Select a code or media file from the file tree.",
    "workspaceFilePreview.loading": "Loading preview.",
    "workspaceFilePreview.unsupportedTitle": "Preview not supported",
    "workspaceFilePreview.unsupportedHint": "This file appears in the file tree, but its content is not read automatically.",
    "workspaceFilePreview.noContent": "No preview content.",
    "workspaceFilePreview.missingTitle": "File was deleted or moved",
    "workspaceFilePreview.video": "video",
    "workspaceFilePreview.image": "image",
    "workspaceFilePreview.mediaBlockedTitle": "{media} file display is blocked",
    "workspaceFilePreview.mediaBlockedHint": "Allow this {media} file to display? Files of the same type will preview directly afterward.",
    "workspaceFilePreview.allowMedia": "Allow {media}",
    "workspaceFilePreview.truncated": "Truncated",
    "workspaceFilePreview.missingError": "File was deleted or moved and cannot be previewed: {path}",
    "workspaceFileTree.collapseDirectory": "Collapse file directory",
    "workspaceFileTree.expandDirectory": "Expand file directory",
    "workspaceFileTree.expandAll": "Expand all",
    "workspaceFileTree.collapseAll": "Collapse all",
    "workspaceToolPanel.expandState": "Expand state",
    "workspaceToolPanel.memorySettingsMenu": "Memory settings menu",
    "workspaceToolPanel.skillsSettingsMenu": "Plugin menu",
    "workspaceToolPanel.agentsSettingsMenu": "Agent settings menu",
    "workspaceToolPanel.menu": "Menu",
    "workspaceToolPanel.collapseToPill": "Collapse to pill",
    "workspaceToolPanel.status": "Status",
    "workspaceShell.skillsTitle": "Plugins",
    "workspaceShell.agentsTitle": "Agent settings",
    "workspaceShell.memoryTitle": "Memory settings",
    "workspaceShell.version": "Version {version}",
    "workspacePanel.resizeRight": "Resize right panel",
    "workspaceContext.switchTabs": "Switch right tabs",
    "workspaceContext.searchTabs": "Search tabs",
    "workspaceContext.searchTabsPlaceholder": "Search tabs...",
    "workspaceContext.clearTabSearch": "Clear tab search",
    "workspaceContext.openTabs": "Open tabs",
    "workspaceContext.closeTab": "Close {title}",
    "workspaceContext.openRightPage": "Open right page",
    "workspaceContext.openPage": "Open page",
    "workspaceContext.launcher.filesDescription": "Browse project files",
    "workspaceContext.launcher.reviewDescription": "Review code changes",
    "workspaceContext.launcher.memoryDescription": "View conversation memory",
    "workspaceContext.loadingReviewPanel": "Loading review panel.",
    "workspaceContext.loadingFilesPanel": "Loading files panel.",
    "workspaceMemory.handleDynamicMemory": "Handle dynamic memory",
    "workspaceMemory.expiredRegionTitle": "Expired",
    "workspaceMemory.expiredRegionDescription": "These memories will be removed after context compaction",
    "workspaceMemory.suppressForSession": "Stop injecting in this session",
    "workspaceMemory.permanentDeleteInvalid": "Permanently delete invalid memory",
    "workspaceMemory.viewDetails": "View details",
    "workspaceMemory.turnId": "Turn ID {turnId}",
    "workspaceMemory.loadingTurn": "Loading full TURN record...",
    "workspaceMemory.lookupFailed": "Lookup failed: {error}",
    "workspaceMemory.user": "User: {content}",
    "workspaceMemory.assistant": "Assistant: {content}",
    "workspaceMemory.viewDetailsWithTurn": "View details {turnId}",
    "workspaceMemory.handleMemoryWithId": "Handle memory {memoryId}",
    "workspaceMemory.suppressedNotice": "Hidden this dynamic memory in the current session.",
    "workspaceMemory.suppressFailed": "Hide failed: {error}",
    "workspaceMemory.deletedNotice": "Memory deleted.",
    "workspaceMemory.deletedWithoutId": "Delete ran, but the service did not return a deleted ID.",
    "workspaceMemory.deleteFailed": "Delete failed: {error}",
    "workspaceReview.refreshGitDiffFailed": "Failed to refresh Git diff: {error}",
    "workspaceReview.noProjectPathForGit": "The current conversation has no project path, so Git operations cannot run.",
    "workspaceReview.gitOperationFailed": "Git operation failed: {error}",
    "workspaceReview.fileDetailTimeout": "Timed out loading file diff details: {error}",
    "workspaceReview.fileDetailFailed": "Failed to load file diff details: {error}",
    "workspaceReview.noProjectPathForRefresh": "The current conversation has no project path, so Git diff cannot refresh.",
    "workspaceReview.fileDetailLoading": "Loading file diff details.",
    "workspaceReview.hunkExpanding": "Expanding hunk context.",
    "workspaceReview.hunkDirectionUp": "upward",
    "workspaceReview.hunkDirectionDown": "downward",
    "workspaceReview.hunkDirectionBoth": "both ways",
    "workspaceReview.hunkExpandingSpecific": "Expanding hunk {index} context {direction}.",
    "workspaceReview.fileDetailTimeoutError": "Single-file diff details took more than {seconds} seconds to load.",
    "workbenchConversation.unpinned": "Unpinned conversation: {conversation}",
    "workbenchConversation.pinned": "Pinned conversation: {conversation}",
    "workbenchConversation.archived": "Archived conversation: {conversation}",
    "workbenchConversation.noProject": "Could not find the project for this conversation.",
    "workbenchConversation.copiedWorkspace": "Copied workspace directory.",
    "workbenchConversation.copiedTitle": "Copied title.",
    "workbenchConversation.copiedSessionId": "Copied session ID.",
    "workbenchConversation.nameEmpty": "Name cannot be empty.",
    "workbenchConversation.projectRenamed": "Renamed project: {project}",
    "workbenchConversation.projectNameRestored": "Restored project name: {project}",
    "workbenchConversation.conversationRenamed": "Renamed conversation: {conversation}",
    "workbenchConversation.archivedCount": "Archived {count} conversations.",
    "workbenchConversation.archivedAndRemovedProject": "Archived and removed project: {project}",
    "workbenchAsk.submitted": "Submitted ask answer.",
    "workbenchAsk.cancelled": "Cancelled ask request.",
    "thinkingDisplay.popupOnly": "Popup",
    "thinkingDisplay.inlineOutput": "Inline output",
    "thinkingDisplay.firstInlineWithPopup": "First inline + popup",
    "thinkingDisplay.firstInlineOnly": "First inline only",
    "ask.title": "ask needs your confirmation",
    "ask.description": "The current AI call is waiting for your answer and will continue after you submit.",
    "ask.action.confirmSubmit": "Confirm submit",
    "ask.action.confirmReject": "Confirm stop",
    "ask.action.submit": "Submit",
    "ask.action.continue": "Continue",
    "ask.action.ignore": "Ignore",
    "ask.action.backToEdit": "Back to edit",
    "ask.action.processing": "Processing...",
    "ask.progress.previous": "Previous question",
    "ask.progress.next": "Next question",
    "ask.option.selected": "Selected",
    "ask.confirm.submitTitle": "Confirm submit?",
    "ask.confirm.rejectTitle": "Stop the current session?",
    "ask.confirm.submitBody": "Press Enter to submit, or Esc to return to editing.",
    "ask.confirm.rejectBody": "Stopping will fail the waiting ask tool call. Press Esc to return to editing.",
    "ask.custom.textPlaceholder": "Enter an answer",
    "ask.custom.adjustCodex": "No, tell Codex how to adjust",
    "ask.custom.description": "Enter a custom answer. Lines wrap automatically; press Shift+Enter for a manual line break.",
    "ask.validation.requiredAnswer": "Please answer \"{header}\" first.",
    "ask.validation.requiredOther": "Please fill in the other answer for \"{header}\" first.",
    "ask.validation.requiredSelectionOrOther": "Please choose an answer for \"{header}\" or fill in another answer.",
    "terminalStop.title": "Force stop the terminal session?",
    "terminalStop.description": "This will stop the running terminal process. Confirm that this session no longer needs to produce output.",
    "terminalStop.stopping": "Stopping",
    "terminalStop.confirm": "Force stop",
    "conversation.bodyLoadFailed": "Conversation body failed to load: {error}",
    "conversation.emptyLoaded": "The conversation body has not loaded yet or has no visible messages.",
    "conversation.emptyWaiting": "No conversation is selected yet. Waiting for the workspace data to load.",
    "conversation.detail.copy": "Copy detail",
    "conversation.detail.close": "Close detail",
    "conversation.userPrompt.expand": "Show more",
    "conversation.userPrompt.collapse": "Collapse",
    "conversation.userPrompt.copy": "Copy user prompt",
    "conversation.userPrompt.edit": "Edit user prompt",
    "conversation.subagent.schedule": "Agent info",
    "conversation.subagent.call": "Create agent",
    "conversation.subagent.calling": "Create agent",
    "conversation.subagent.state.running": "Running",
    "conversation.subagent.state.stopped": "Stopped",
    "conversation.subagent.state.failed": "Failed",
    "conversation.subagent.state.completed": "Standby",
    "conversation.subagent.detail.createdTask": "Created agent task",
    "conversation.subagent.detail.name": "Name",
    "conversation.subagent.detail.description": "Description",
    "conversation.subagent.detail.prompt": "Prompt",
    "conversation.subagent.detail.effectivePrompt": "Effective prompt",
    "conversation.subagent.detail.debug": "Debug",
    "conversation.subagent.detail.empty": "No content.",
    "conversation.subagent.detail.message": "Send message to agent",
    "conversation.subagent.detail.wait": "Waiting for agent message",
    "conversation.subagent.detail.waitPending": "Waiting",
    "conversation.subagent.detail.waitCompleted": "Agent message received",
    "conversation.subagent.detail.waitInterrupted": "Wait interrupted",
    "conversation.subagent.detail.waitTimedOut": "Wait timed out",
    "conversation.subagent.detail.templateCollection": "Agent template list",
    "conversation.subagent.detail.statusCollection": "Agent status list",
    "conversation.subagent.detail.feedback": "Agent returned result",
    "conversation.subagent.detail.openFeedback": "Click to expand",
    "conversation.subagent.detail.count": "Count",
    "conversation.subagent.detail.completed": "Completed",
    "conversation.subagent.detail.timeout": "Timeout",
    "conversation.subagent.detail.noAgents": "No visible agent.",
    "conversation.subagent.detail.noOpenAgents": "No open agents.",
    "conversation.subagent.enabled": "Enabled",
    "conversation.subagent.disabled": "Disabled",
    "conversation.subagent.detail.agent": "Agent",
    "conversation.subagent.detail.status": "Status",
    "conversation.subagent.detail.unknown": "Unknown",
    "conversation.subagent.detail.result": "Result",
    "conversation.subagent.detail.noResult": "No result content.",
    "conversation.subagent.detail.target": "Target",
    "conversation.subagent.detail.queued": "Queued",
    "conversation.subagent.detail.deliveryMode": "Delivery mode",
    "conversation.subagent.detail.targetSessions": "Target sessions",
    "conversation.subagent.detail.note": "Note",
    "conversation.subagent.detail.closeControl": "Close agent",
    "conversation.subagent.detail.interruptControl": "Stop agent",
    "conversation.subagent.detail.stoppedCount": "Stopped count",
    "conversation.subagent.detail.stoppedSessions": "Stopped sessions",
    "conversation.subagent.detail.closedCount": "Closed count",
    "conversation.subagent.detail.closedAgents": "Closed agents",
    "conversation.subagent.detail.clearedMessages": "Cleared messages",
    "conversation.subagent.detail.yes": "Yes",
    "conversation.subagent.detail.no": "No",
    "conversation.subagent.closeTask": "End agent",
    "conversation.subagent.loadingTranscript": "Loading agent messages",
    "conversation.subagent.closeConfirmTitle": "End this agent?",
    "conversation.subagent.closeConfirmDescription": "This will end \"{task}\" and clear queued messages. Ended agents cannot receive more messages.",
    "conversation.subagent.closeConfirm": "End agent",
    "conversation.subagent.closing": "Ending",
    "conversation.subagent.stopConfirmTitle": "Stop this agent?",
    "conversation.subagent.stopConfirmDescription": "This will stop only \"{task}\" and will not close the agent, stop the main conversation, or stop other agents.",
    "conversation.subagent.stopConfirm": "Stop agent",
    "conversation.subagent.stopping": "Stopping",
    "conversation.diff.loadingPanel": "Loading the Git Diff review panel.",
    "conversation.diff.parsingWorker": "Parsing diff content in a Worker.",
    "conversation.diff.parseFailed": "Diff parse failed: {error}",
    "conversation.diff.unstructuredFallback": "This diff content cannot be parsed into a structured view. Raw text is still available for copying.",
    "sessionTools.expand": "Expand session tools",
    "sessionTools.collapse": "Collapse session tools",
    "sessionTools.changes": "Changes",
    "sessionTools.progress": "Progress",
    "sessionTools.terminal": "Terminal",
    "sessionTools.menu": "Session tools menu",
    "terminalSession.openScreen": "Open terminal screen",
    "terminalSession.noScreen": "No terminal screen",
    "terminalSession.forceStop": "Force stop",
    "terminalSession.forceStopSession": "Force stop terminal session",
    "terminalSession.emptyScreen": "No terminal screen content",
    "terminalSession.tooltip.status": "Status",
    "terminalSession.tooltip.command": "Command",
    "terminalSession.tooltip.cwd": "Directory",
    "terminalSession.tooltip.shell": "Shell",
    "terminalSession.tooltip.backend": "Backend",
    "terminalSession.tooltip.screenFrame": "Screen frame",
    "terminalSession.tooltip.exitCode": "Exit code",
    "terminalSession.tooltip.error": "Error",
    "terminalSession.tooltip.output": "Output",
    "terminalSession.tooltip.session": "Session",
    "terminalState.running": "Running",
    "terminalState.waiting": "Waiting for input",
    "terminalState.success": "Completed",
    "terminalState.failed": "Failed",
    "terminalState.stopped": "Stopped",
    "terminalState.lost": "Lost",
    "terminalState.unknown": "Unknown",
    "toolPresentation.output.loading": "Loading output...",
    "toolPresentation.output.empty": "No output",
    "toolPresentation.exitCode": "Exit code",
    "toolPresentation.command.running": "Running",
    "toolPresentation.command.failed": "Failed",
    "toolPresentation.command.stopped": "Stopped",
    "toolPresentation.command.completed": "Completed",
    "toolPresentation.command.start.failed": "Start failed",
    "toolPresentation.command.read.failed": "Read failed",
    "toolPresentation.command.send.failed": "Send failed",
    "toolPresentation.command.write.failed": "Write failed",
    "toolPresentation.command.stop.failed": "Stop failed",
    "toolPresentation.command.start.running": "Starting",
    "toolPresentation.command.read.running": "Reading",
    "toolPresentation.command.send.running": "Sending",
    "toolPresentation.command.write.running": "Writing",
    "toolPresentation.command.stop.running": "Stopping",
    "toolPresentation.command.start.completed": "Started",
    "toolPresentation.command.list.completed": "Listed",
    "toolPresentation.command.read.completed": "Read",
    "toolPresentation.command.send.completed": "Sent",
    "toolPresentation.command.write.completed": "Written",
    "toolPresentation.command.stop.completed": "Stopped",
    "toolPresentation.command.heading.start": "Start command",
    "toolPresentation.command.heading.read": "Read output",
    "toolPresentation.command.heading.send": "Send input",
    "toolPresentation.command.heading.write": "Write input",
    "toolPresentation.command.heading.stop": "Stop session",
    "toolPresentation.command.session": "Command session",
    "toolPresentation.command.lost": "live status lost",
    "toolPresentation.command.current": "current",
    "toolPresentation.command.currentState": "current {state}",
    "toolPresentation.lua.title": "Execute Lua code",
    "toolPresentation.lua.running": "Running",
    "toolPresentation.lua.failed": "Failed",
    "toolPresentation.lua.stopped": "Stopped",
    "toolPresentation.lua.completed": "Executed",
    "toolPresentation.lua.action.eval": "Execute Lua code",
    "toolPresentation.lua.action.invalid": "Lua session operation failed",
    "toolPresentation.lua.action.list": "List Lua sessions",
    "toolPresentation.lua.action.start": "Start persistent Lua session",
    "toolPresentation.lua.action.status": "Read Lua session status",
    "toolPresentation.lua.action.close": "Close Lua session",
    "toolPresentation.lua.oneShot": "Isolated execution",
    "toolPresentation.lua.persistent": "Persistent session",
    "toolPresentation.lua.session": "Session",
    "toolPresentation.lua.ttl": "Idle TTL",
    "toolPresentation.lua.elapsed": "Elapsed",
    "toolPresentation.lua.code": "Lua code",
    "toolPresentation.lua.arguments": "Structured arguments",
    "toolPresentation.lua.output": "Result",
    "toolPresentation.lua.argumentName": "Parameter",
    "toolPresentation.lua.argumentValue": "Value",
    "toolPresentation.lua.sessionName": "Name",
    "toolPresentation.lua.sessionId": "Session ID",
    "toolPresentation.lua.sessionState": "State",
    "toolPresentation.lua.sessionTtl": "Idle TTL",
    "toolPresentation.lua.sessionExpiresAt": "Expires at",
    "toolPresentation.lua.sessionActive": "Active",
    "toolPresentation.lua.sessionClosed": "Closed",
    "toolPresentation.lua.sessionExpired": "Expired",
    "toolPresentation.lua.sessionFailed": "Unavailable",
    "toolPresentation.lua.noSessions": "There are no persistent Lua sessions.",
    "toolPresentation.lua.closeSucceeded": "Lua session `{session}` was closed.",
    "toolPresentation.ask.running": "Asking",
    "toolPresentation.ask.ignored": "Ignored",
    "toolPresentation.ask.completed": "Asked",
    "toolPresentation.ask.questionLabel": "Question:",
    "toolPresentation.ask.answerLabel": "Answer:",
    "toolPresentation.ask.waitingAnswer": "Waiting for user answer",
    "toolPresentation.file.count": "{count} files",
    "toolPresentation.file.multiChange": "Multiple file changes",
    "toolPresentation.file.empty": "No displayable content",
    "toolPresentation.file.create.running": "Writing",
    "toolPresentation.file.modify.running": "Editing",
    "toolPresentation.file.delete.running": "Deleting",
    "toolPresentation.file.mixed.running": "Changing",
    "toolPresentation.file.create.failed": "Write failed",
    "toolPresentation.file.modify.failed": "Edit failed",
    "toolPresentation.file.delete.failed": "Delete failed",
    "toolPresentation.file.mixed.failed": "Change failed",
    "toolPresentation.file.create.preview": "Preview write",
    "toolPresentation.file.modify.preview": "Preview edit",
    "toolPresentation.file.delete.preview": "Preview delete",
    "toolPresentation.file.mixed.preview": "Preview change",
    "toolPresentation.file.create.completed": "Written",
    "toolPresentation.file.modify.completed": "Edited",
    "toolPresentation.file.delete.completed": "Deleted",
    "toolPresentation.file.mixed.completed": "Changed",
    "toolPresentation.markdownOutput.running": "Querying",
    "toolPresentation.markdownOutput.failed": "Query failed",
    "toolPresentation.markdownOutput.stopped": "Stopped",
    "toolPresentation.markdownOutput.completed": "Queried",
    "toolPresentation.fileList.title": "List files",
    "toolPresentation.fileRead.title": "Read file",
    "toolPresentation.codekit.astTree.title": "List AST Tree",
    "toolPresentation.codekit.astDetail.title": "View AST Details",
    "toolPresentation.codekit.rg.title": "RG Enhanced Search",
    "toolPresentation.codekit.nodeSource.title": "Get node source",
    "markdown.wrap.enable": "Enable line wrap",
    "markdown.wrap.disable": "Disable line wrap",
    "markdown.copyCode": "Copy code",
    "markdown.copied": "Copied",
    "mediaPlayer.maximize": "Maximize player",
    "mediaPlayer.restore": "Restore player",
    "mediaPlayer.close": "Close player",
    "mediaPlayer.fitWindow": "Fit to window",
    "mediaPlayer.originalSize": "Original size",
    "mediaPlayer.zoomOut": "Zoom out",
    "mediaPlayer.zoomIn": "Zoom in",
    "media.openVideoPlayer": "Open video player",
    "settings.backToWorkspace": "Back to workspace",
    "memory.scope": "Scope",
    "memory.scope.global": "Global config",
    "memory.scope.project": "Project config",
    "memory.project": "Project",
    "memory.action.testConnection": "Test connection",
    "memory.action.refreshUsersProjects": "Refresh users/projects",
    "memory.vmm.action.running": "Running VMM action: {action}",
    "memory.vmm.action.failed": "VMM action failed: {message}",
    "memory.vmm.project.loading": "Loading project VMM config: {project}.",
    "memory.vmm.project.selected": "Editing project: {project}.",
    "memory.vmm.save.global.running": "Saving global VMM config.",
    "memory.vmm.save.project.running": "Saving project VMM config.",
    "memory.vmm.save.global.completed": "Global VMM config saved.",
    "memory.vmm.save.project.completed": "Project VMM config saved.",
    "memory.vmm.save.failed": "Failed to save VMM config: {message}",
    "vmm.common.optional": "Optional",
    "vmm.common.number": "Number",
    "vmm.common.enabled": "Enabled",
    "vmm.common.disabled": "Off",
    "vmm.common.none": "None",
    "vmm.common.notConfigured": "Not configured",
    "vmm.common.notSelected": "Not selected",
    "vmm.common.running": "Running",
    "vmm.overview.effectiveConfig": "Effective config",
    "vmm.overview.grpc": "gRPC: ",
    "vmm.overview.recallPolicy": "Recall policy",
    "vmm.overview.ttlTurns": "TTL: {turns} turns",
    "vmm.overview.precheckAiTools": "Precheck recall: {precheck} · AI tools: {tools}",
    "vmm.overview.writeback": "Writeback: {writeback}",
    "vmm.overview.runtime": "Runtime",
    "vmm.overview.outboxProfiles": "outbox: {outbox} · profile nodes: {profiles}",
    "vmm.overview.completedTurns": "Completed turns: {turns}",
    "vmm.overview.summaryEntry": "Summary entry",
    "vmm.overview.noSummary": "No summaries",
    "vmm.runtime.title": "Runtime",
    "vmm.runtime.subtitle": "VMM hooks and profile cache for the current session.",
    "vmm.runtime.session": "Session",
    "vmm.runtime.profileNodes": "Profile nodes",
    "vmm.runtime.updatedAt": "Updated at",
    "vmm.runtime.noProfileBase": "No profile base yet.",
    "vmm.result.title": "Action result",
    "vmm.result.emptySummary": "No VMM action has run yet.",
    "vmm.result.request": "Request",
    "vmm.result.response": "Response",
    "vmm.result.emptyHint": "Requests and responses appear after you run an action on the left.",
    "vmm.result.generic": "Result",
    "vmm.settings.grpcTarget": "gRPC address",
    "vmm.settings.apiKey": "API Key",
    "vmm.settings.receiveTimeout": "Request timeout",
    "vmm.settings.handshakeTimeout": "Handshake timeout",
    "vmm.settings.maxReceiveBytes": "Max receive bytes",
    "vmm.settings.keepaliveInterval": "keepalive interval",
    "vmm.settings.keepaliveTimeout": "keepalive timeout",
    "vmm.settings.idleKeepalive": "Idle keepalive",
    "vmm.settings.enableMemoryConnection": "Enable memory connection",
    "vmm.settings.exposeAiTools": "Expose AI-only tools",
    "vmm.settings.autoPrecheck": "Automatic precheck recall",
    "vmm.settings.precheckPolicy": "Precheck memory injection",
    "vmm.settings.writebackPolicy": "Post-turn memory writeback",
    "vmm.settings.policy.inherit": "Follow parent",
    "vmm.settings.policy.enabled": "On",
    "vmm.settings.policy.disabled": "Off",
    "vmm.settings.userId": "User ID",
    "vmm.settings.projectId": "Project ID",
    "vmm.settings.implicitMemoryTtl": "Temporary memory TTL",
    "vmm.settings.profileRefreshInterval": "Profile refresh interval",
    "vmm.settings.profileNodeLimit": "Profile node limit",
    "vmm.settings.injectionTag": "Injection tag",
    "vmm.settings.injectionDescription": "Injection description",
    "vmm.rows.usersEmpty": "User rows appear after refreshing users/projects.",
    "vmm.rows.projectsEmpty": "Project rows appear after refreshing users/projects.",
    "vmm.rows.profileNodesEmpty": "Active profile nodes appear here after refreshing profile nodes.",
    "vmm.rows.bundleEmpty": "Bundle content appears after loading the full profile.",
    "vmm.rows.toolResultEmpty": "Tool debug results appear by action.",
    "vmm.rows.unnamedUser": "Unnamed user",
    "vmm.rows.unnamedProject": "Unnamed project",
    "vmm.rows.deleted": "Deleted",
    "vmm.rows.notFound": "Not found",
    "vmm.rows.vectorRows": "Vector rows {count}",
    "vmm.profile.full": "Full profile",
    "vmm.profile.team": "Team profile",
    "vmm.profile.space": "Space profile",
    "vmm.profile.project": "Project profile",
    "vmm.profile.user": "User profile",
    "vmm.action.deleteMemory": "Delete memory",
    "vmm.action.precheckProject": "Precheck project",
    "vmm.action.confirmCreate": "Confirm create",
    "vmm.action.deleteProject": "Delete project",
    "vmm.action.runMigrate": "Run migration",
    "vmm.action.refreshNodes": "Refresh nodes",
    "vmm.action.applyInstruction": "Apply instruction",
    "vmm.action.loadProfileBundle": "Load full profile",
    "vmm.action.saveGlobal": "Save globally",
    "vmm.action.saveProject": "Save to project",
    "vmm.action.resolveUser": "Resolve user",
    "vmm.action.deleteUser": "Delete user",
    "vmm.section.project.title": "Project management",
    "vmm.section.project.subtitle": "Precheck project paths, confirm creation, confirm deletion, and confirm migration.",
    "vmm.section.profile.title": "Profile center",
    "vmm.section.profile.subtitle": "Read active profile nodes and submit manual profile instructions to the current target.",
    "vmm.section.bundle.title": "Profile bundle test",
    "vmm.section.bundle.subtitle": "Read the current user/project FULL bundle and inspect the combined profile.",
    "vmm.section.tool.title": "Tool debug",
    "vmm.section.tool.subtitle": "Simulate VMM AI-only tools: search memory, expand turns, and write memory.",
    "vmm.section.memory.title": "Memory settings",
    "vmm.section.memory.subtitle": "Configure memory enablement, AI tools, temporary TTL, profile refresh, and injection notes.",
    "vmm.section.transport.title": "gRPC transport",
    "vmm.section.transport.subtitle": "Connection address, API key, timeouts, max receive bytes, and keepalive policy.",
    "vmm.section.user.title": "User management",
    "vmm.section.user.subtitle": "Long-term user list, user reference resolution, creation confirmation, and deletion confirmation.",
    "vmm.field.projectPath": "Project path",
    "vmm.field.confirmDelete": "Confirm delete",
    "vmm.field.migrateSource": "Migration source project",
    "vmm.field.migrateTarget": "Migration target project",
    "vmm.field.confirmMigrate": "Confirm migration",
    "vmm.field.profileTarget": "Profile target",
    "vmm.field.profileInstruction": "Manual profile instruction",
    "vmm.field.searchQueries": "Search query",
    "vmm.field.deleteReason": "Delete reason",
    "vmm.field.writeAbstract": "Manual write summary",
    "vmm.field.writeDetails": "Manual write details",
    "vmm.field.userRef": "User reference",
    "vmm.field.deleteConfirmationCode": "Delete confirmation code",
    "vmm.placeholder.profileInstruction": "Example: reduce the weight of a preference",
    "vmm.placeholder.multiPipe": "Separate multiple values with |",
    "vmm.placeholder.commaOrPipe": "Separate with commas or |",
    "vmm.placeholder.deleteReason": "Wrong, expired, or requested by the user",
    "vmm.placeholder.writeAbstract": "Memory summary",
    "vmm.placeholder.writeDetails": "Full content",
    "vmm.placeholder.userRef": "ID or username",
    "vmm.placeholder.deleteConfirmationCode": "Returned after the first delete",
    "vmm.diagnostic.disabledByConfig": "Disabled",
    "vmm.diagnostic.missingGrpcTarget": "Missing gRPC",
    "vmm.diagnostic.grpcOffline": "gRPC offline",
    "vmm.diagnostic.missingUserId": "Missing user",
    "vmm.diagnostic.missingProjectId": "Missing project",
    "planArtifact.titleFallback": "Plan",
    "planArtifact.copy": "Copy Markdown",
    "planArtifact.copied": "Copied Markdown",
    "planArtifact.expand": "Expand plan",
    "planArtifact.collapse": "Collapse plan",
    "planArtifact.confirmAria": "Plan confirmation actions",
    "planArtifact.noteAria": "Plan action note",
    "planArtifact.notePlaceholder": "Additional note (optional)",
    "planArtifact.execute": "Execute plan",
    "planArtifact.executeWithNote": "Add note and execute plan",
    "planArtifact.defer": "Not now",
    "planArtifact.suggest": "Send plan note",
    "runtime.memoryRecall": "✨ Recalling memory",
    "runtime.requesting": "✨ Requesting",
    "runtime.toolCalling": "🔧 Building tool request",
    "runtime.thinking": "✨ Thinking",
    "runtime.thinkingOverlayDockRight": "Switch to the compact right thinking panel",
    "runtime.thinkingOverlayRestoreCentered": "Restore the bottom-centered thinking panel",
    "settings.general": "General",
    "settings.systemSettings": "System settings",
    "settings.projects": "Projects",
    "settings.providers": "Models",
    "settings.tools": "Tools",
    "settings.diagnostics": "Diagnostics",
    "settings.general.description": "Runtime, remote access, and interface settings",
    "settings.projects.description": "Workspace project list and bindings",
    "settings.providers.description": "Providers, models, reasoning, and tool capability",
    "settings.tools.description": "Tool calls, command sessions, and output",
    "settings.diagnostics.description": "Startup logs, errors, and runtime events",
    "settings.group.models": "Model settings",
    "settings.group.aiCapabilities": "AI capabilities",
    "settings.modelProviders": "Model providers",
    "settings.modelList": "Model list",
    "settings.providerRetry": "Request retry",
    "settings.aiVendors": "Capability vendors",
    "settings.aiWebSearch": "Web search",
    "settings.aiUnderstandImage": "Image understanding",
    "settings.aiUnderstandVideo": "Video understanding",
    "settings.aiUnderstandAudio": "Audio understanding",
    "settings.aiImageGeneration": "Image generation",
    "settings.aiVideoGeneration": "Video generation",
    "settings.aiSpeechSynthesis": "Speech synthesis",
    "settings.aiMusicGeneration": "Music generation",
    "settings.aiMusicCover": "Music cover",
    "settings.mcpServers": "MCP servers",
    "settings.runtimeDiagnostics": "Runtime diagnostics",
    "settings.gitCommit": "Git commit",
    "settings.promptComposer": "Prompt composer",
    "settings.aboutLicenses": "About and licenses",
    "settings.archivedRestore": "Archived restore",
    "settings.modelProviders.description": "Chat model providers, current model, and enabled model status.",
    "settings.modelList.description": "Review enabled models by provider and the active workbench model.",
    "settings.providerRetry.description": "Configure retry counts, wait intervals, and backoff rules for ordinary failures and 429 rate limits.",
    "settings.aiVendors.description": "Capability vendors and route targets shared by semantic AI tools.",
    "settings.aiWebSearch.description": "Web-search capability state and available targets.",
    "settings.aiUnderstandImage.description": "Image-understanding capability state and available targets.",
    "settings.aiUnderstandVideo.description": "Video-understanding capability entry and connection status.",
    "settings.aiUnderstandAudio.description": "Audio-understanding capability entry and connection status.",
    "settings.aiImageGeneration.description": "Image-generation capability state and available targets.",
    "settings.aiVideoGeneration.description": "Video-generation capability state and available targets.",
    "settings.aiSpeechSynthesis.description": "Speech-synthesis capability state and available targets.",
    "settings.aiMusicGeneration.description": "Music-generation capability state and available targets.",
    "settings.aiMusicCover.description": "Music-cover capability state and available targets.",
    "settings.mcpServers.description": "MCP runtime, server connections, tools, prompts, and resources.",
    "settings.runtimeDiagnostics.description": "Startup flow, runtime modules, and diagnostic logs.",
    "settings.diagnostics.logs": "Diagnostic logs",
    "settings.diagnostics.startupStatus": "Startup status",
    "settings.diagnostics.waitingStatus": "Waiting for status update",
    "settings.diagnostics.runtimeModules": "Runtime modules",
    "settings.gitCommit.description": "New-branch naming, automatic summary model, and extra prompt text for the commit dialog.",
    "settings.gitCommit.active": "Active",
    "settings.gitCommit.autoSummary": "Auto summary",
    "settings.gitCommit.summaryModel": "Commit summary model",
    "settings.gitCommit.followSessionModel": "Follow the owning session model",
    "settings.gitCommit.chooseSummaryModel": "Choose commit summary model",
    "settings.gitCommit.resetSummaryModel": "Follow session again",
    "settings.gitCommit.followSessionModelHelp": "When no model is fixed, automatic summaries use the current model from the session that starts the commit action.",
    "settings.gitCommit.fixedSummaryModelHelp": "When a model is fixed, automatic summaries prefer that model; unavailable models are rejected or must be reselected.",
    "settings.gitCommit.fixedSummaryModelUnavailable": "This fixed model is not available in the current snapshot. Choose another model or follow the session model.",
    "settings.gitCommit.summaryExtraPrompt": "Extra summary prompt",
    "settings.gitCommit.summaryExtraPromptPlaceholder": "Example: write the commit message in Chinese; keep the title under 50 characters; split the body into bullet lines.",
    "settings.gitCommit.summaryExtraPromptHelp": "This text is appended after the fixed English base prompt to express your commit-message preferences.",
    "gitCommitDialog.generateEmpty": "No commit message was generated.",
    "gitCommitDialog.generated": "Commit message generated, about",
    "gitCommitDialog.generatedTruncated": "Commit message generated with truncated diff input, about",
    "gitCommitDialog.actionDone": "Git action completed.",
    "gitCommitDialog.pushDone": "Git push completed.",
    "gitCommitDialog.generating": "Generating",
    "gitCommitDialog.preparingCommitContent": "Preparing changes",
    "gitCommitDialog.generatingCommitMessage": "Generating commit message",
    "gitCommitDialog.committing": "Committing",
    "gitCommitDialog.pushingDetailed": "Pushing",
    "gitCommitDialog.running": "Running",
    "gitCommitDialog.pushing": "Pushing",
    "gitCommitDialog.toolsTitle": "Git tools",
    "gitCommitDialog.changes": "Changes",
    "gitCommitDialog.open": "Commit or push",
    "gitCommitDialog.title": "Commit or push",
    "gitCommitDialog.description": "Choose a commit target, enter a commit message, and run commit actions based on the current Git changes.",
    "gitCommitDialog.currentBranch": "Current branch",
    "gitCommitDialog.newBranch": "New branch",
    "gitCommitDialog.recommendedPrefix": "Recommended: ",
    "gitCommitDialog.noChanges": "No changes",
    "gitCommitDialog.branchName": "New branch name",
    "gitCommitDialog.message": "Commit message",
    "gitCommitDialog.messagePlaceholder": "Commit message (leave empty to generate one)...",
    "gitCommitDialog.includeUnstaged": "Include unstaged changes",
    "gitCommitDialog.commit": "Commit",
    "gitCommitDialog.commitAndPush": "Commit and push",
    "gitCommitDialog.push": "Push",
    "gitCommitDialog.commitTo": "Commit to",
    "settings.promptComposer.description": "Composer slash commands, attachment capability, and reasoning selection.",
    "settings.aboutLicenses.description": "Application version, license entry points, and compliance notes.",
    "settings.archivedRestore.description": "Archived project and conversation restore entry.",
    "skills.action.refresh": "Refresh",
    "skills.action.refreshing": "Refreshing",
    "skills.action.installLua": "Install LuaSkills",
    "skills.action.install": "Install",
    "skills.action.installing": "Installing",
    "skills.action.cancel": "Cancel",
    "skills.action.details": "Details",
    "skills.action.tools": "Tools",
    "skills.action.action": "Actions",
    "skills.action.enable": "Enable",
    "skills.action.disable": "Disable",
    "skills.action.update": "Update",
    "skills.action.delete": "Delete",
    "skills.status.enabled": "Enabled",
    "skills.status.stopped": "Disabled",
    "skills.filter.searchLabel": "Search list",
    "skills.filter.searchPlaceholder": "Search names, descriptions, identifiers, or sources",
    "skills.filter.statusLabel": "Filter by status",
    "skills.filter.allStatuses": "All statuses",
    "skills.filter.issues": "Has issues",
    "skills.filter.sourceLabel": "Filter by source",
    "skills.filter.allSources": "All sources",
    "skills.filter.empty": "No items match the current filters.",
    "skills.label.system": "System",
    "skills.label.user": "User",
    "skills.label.source": "Source",
    "skills.label.explicit": "Explicit",
    "skills.label.implicit": "Implicit",
    "skills.label.closed": "Closed",
    "skills.label.currentStatus": "Current Status",
    "skills.label.installLayer": "Install Layer",
    "skills.label.installManagement": "Install Management",
    "skills.label.runtimeEntries": "Runtime Entries",
    "skills.label.runtimeTools": "Runtime Tools",
    "skills.label.managedInstall": "Managed Install",
    "skills.label.unmanagedInstall": "Unmanaged Install",
    "skills.label.scope": "Scope",
    "skills.label.skillFile": "Skill File",
    "skills.label.skillDir": "Skill Directory",
    "skills.source.unknown": "No source recorded",
    "skills.sidebar.luaDetail": "{count} skill packages",
    "skills.navigation.list": "LuaSkills Skills",
    "skills.navigation.plugins": "Plugin Management",
    "skills.navigation.rtkSettings": "RTK Settings",
    "skills.navigation.luaSettings": "LuaSkills Settings",
    "skills.luaSettings.description": "These settings are owned by Vulcan and configure both the conversation and service LuaSkills engines.",
    "skills.luaSettings.restartRequired": "Saved; restart required",
    "skills.luaSettings.restartHint": "The RunLua AI tool switch applies immediately. VM pool, network, runtime, and capability changes still require restarting Vulcan Code.",
    "skills.luaSettings.saved": "Vulcan LuaSkills configuration saved.",
    "skills.luaSettings.save": "Save LuaSkills Configuration",
    "skills.luaSettings.saving": "Saving…",
    "skills.luaSettings.aiTools": "AI Tools",
    "skills.luaSettings.aiToolsHint": "Controls AI-visible tools only; LuaSkills internals and plugin-owned Lua sessions remain available.",
    "skills.luaSettings.enableRunLua": "runlua",
    "skills.luaSettings.enableRunLuaSession": "runlua-session",
    "skills.luaSettings.runLuaTooltip": "Runs one isolated Lua snippet for a calculation, transformation, or script task. Lua state is discarded after the call.",
    "skills.luaSettings.runLuaSessionTooltip": "Enables persistent Lua sessions that retain variables, modules, and intermediate values across calls. This controls the runlua_session AI tool.",
    "skills.luaSettings.mainPool": "Main VM Pool",
    "skills.luaSettings.runLuaPool": "Isolated runLua VM Pool",
    "skills.luaSettings.poolHint": "The minimum controls warm capacity, the maximum limits concurrent VMs, and idle time retires excess VMs.",
    "skills.luaSettings.minSize": "Minimum VMs",
    "skills.luaSettings.maxSize": "Maximum VMs",
    "skills.luaSettings.idleTtl": "Idle Retirement (seconds)",
    "skills.luaSettings.toolCache": "Tool Cache",
    "skills.luaSettings.maxEntries": "Maximum Entries",
    "skills.luaSettings.defaultTtl": "Default TTL (seconds)",
    "skills.luaSettings.maxTtl": "Maximum TTL (seconds)",
    "skills.luaSettings.network": "Network and Sources",
    "skills.luaSettings.allowDownloads": "Allow skill and dependency downloads",
    "skills.luaSettings.githubBaseUrl": "GitHub Website Base URL",
    "skills.luaSettings.githubApiBaseUrl": "GitHub API Base URL",
    "skills.luaSettings.skillHubBaseUrl": "LuaSkills Hub Base URL",
    "skills.luaSettings.privateUrlInstall": "Allow trusted system operations to install private URL manifests",
    "skills.luaSettings.privateAllowlist": "Private Source Allowlist",
    "skills.luaSettings.privateAllowlistHint": "Enter one allowed URL prefix per line; blank lines are removed when saving.",
    "skills.luaSettings.capabilities": "Runtime Capabilities",
    "skills.luaSettings.defaultEncoding": "Default Text Encoding",
    "skills.luaSettings.autoEncoding": "Automatic (runtime default)",
    "skills.luaSettings.skillManagementBridge": "Allow Lua to call the host skill-management bridge",
    "skills.luaSettings.managedIoCompat": "Replace native global IO with the managed IO compatibility layer",
    "skills.luaSettings.managedRuntime": "Managed Python / Node Runtime",
    "skills.luaSettings.managedRuntimeHint": "These values control per-engine workers, persistent sessions, and invoke defaults. Restart after saving.",
    "skills.luaSettings.managedRuntimeStatusReady": "Ready",
    "skills.luaSettings.managedRuntimeStatusInstalled": "Installed during this run",
    "skills.luaSettings.managedRuntimeStatusUnsupported": "Unsupported on this platform",
    "skills.luaSettings.managedRuntimeStatusUnavailable": "Installation unavailable",
    "skills.luaSettings.managedRuntimeRetrying": "Retrying download…",
    "skills.luaSettings.managedRuntimeRetryCompleted": "Managed runtime bootstrap finished. Review the latest status.",
    "skills.luaSettings.pythonRuntime": "Python Runtime",
    "skills.luaSettings.nodeRuntime": "Node.js Runtime",
    "skills.luaSettings.managedRuntimeInstalled": "Installed",
    "skills.luaSettings.managedRuntimeReinstall": "Reinstall",
    "skills.luaSettings.managedRuntimePreparing": "Preparing installation…",
    "skills.luaSettings.combinedRuntime": "Python / Node Runtimes",
    "skills.luaSettings.managedRuntimeNormal": "Normal",
    "skills.luaSettings.managedWorkerPoolMax": "Maximum Workers per Environment",
    "skills.luaSettings.managedWorkerIdleTtl": "Worker Idle Retirement (seconds)",
    "skills.luaSettings.managedSessionLimit": "Persistent Sessions per Engine",
    "skills.luaSettings.managedSessionBuffer": "Default Bytes per Output Stream",
    "skills.luaSettings.managedInvokeTimeout": "Default Invoke Timeout (milliseconds)",
    "skills.luaSettings.unlimited": "Unlimited",
    "skills.luaSettings.validationPositive": "Pool sizes, TTL values, and idle time must be positive integers.",
    "skills.luaSettings.validationPoolOrder": "The maximum VM count cannot be smaller than the minimum for either pool.",
    "skills.luaSettings.validationCacheOrder": "The maximum cache TTL cannot be smaller than the default cache TTL.",
    "skills.sidebar.agentDetail": "{count} skills",
    "skills.sidebar.pluginDetail": "{count} Vulcan plugins",
    "skills.header.luaDetail": "Total skills {total} · Visible tools {visible} · Implicit tools {implicit}",
"skills.header.pluginDetail": "Vulcan plugins {total} · Enabled {enabled}",
    "skills.header.rtkDetail": "Command routing, Git context compression, and token-saving statistics",
    "agents.header.detail": "Total agents {total} · Enabled {enabled} · Load warnings {warnings}",
    "skills.lua.emptyTitle": "Waiting for LuaSkills catalog",
    "skills.lua.emptyDetail": "Real skill packages appear after startup completes or refresh runs.",
    "skills.lua.installTitle": "Install LuaSkills",
    "skills.lua.installDescription": "Install a Lua Skill package from a GitHub repository or URL.",
    "skills.lua.sourceType": "Source Type",
    "skills.lua.installSource": "Install Source",
    "skills.lua.placeholderGithub": "LuaSkills/vulcan-codekit",
    "skills.lua.placeholderUrl": "https://example.com/skill-source.yaml",
    "skills.lua.detailsTitle": "Lua Skill Details",
    "skills.lua.detailsDescription": "View Lua Skill package details.",
    "skills.lua.toolsTitle": "LuaSkills Tools",
    "skills.lua.toolsDescription": "Manage Lua Skill tool visibility.",
    "skills.lua.toolsEmpty": "This skill has no manageable runtime tools yet.",
    "skills.lua.toolModeAria": "{name} tool enablement mode",
    "skills.lua.toolExplicit": "Explicit",
    "skills.lua.toolImplicit": "Implicit",
    "skills.lua.toolDisabled": "Disable",
    "skills.lua.syncing": "Syncing",
    "skills.luaConfig.title": "Package Configuration",
    "skills.luaConfig.description": "Manage configuration from the package declarations.",
    "skills.luaConfig.loading": "Loading package configuration…",
    "skills.luaConfig.empty": "This package declares no configuration items.",
    "skills.luaConfig.complete": "Complete",
    "skills.luaConfig.incomplete": "Incomplete",
    "skills.luaConfig.required": "Required",
    "skills.luaConfig.sensitive": "Sensitive",
    "skills.luaConfig.restartRequired": "Restart required",
    "skills.luaConfig.deprecated": "Deprecated",
    "skills.luaConfig.secretConfigured": "Configured; leaving this blank preserves the current value.",
    "skills.luaConfig.preserveSecret": "Preserve current sensitive value",
    "skills.luaConfig.selectValue": "Select a value",
    "skills.luaConfig.deleteValue": "Delete explicit configuration value",
    "skills.luaConfig.reload": "Reload latest configuration",
    "skills.luaConfig.advanced": "Advanced configuration",
    "skills.luaConfig.orphaned": "Orphaned configuration keys",
    "skills.luaConfig.externalChange": "An external configuration change was detected. The latest revision was loaded and unsaved edits were preserved.",
    "skills.luaConfig.revisionConflict": "Another runtime changed this configuration. The latest revision was loaded and unsaved edits were preserved; review and save again.",
    "skills.luaConfig.saving": "Saving…",
    "skills.luaConfig.save": "Save configuration",
    "skills.plugins.emptyTitle": "No Vulcan plugins",
    "skills.plugins.emptyDetail": "Bundled Vulcan plugins appear here.",
    "skills.plugins.loadingPage": "Loading plugin page…",
    "skills.plugins.externalUrlConfirmation": "This external link is not allowlisted. You can still open it if you trust it.",
    "skills.plugins.externalUrlOpenAnyway": "Open anyway",
    "skills.plugins.externalUrlOpenFailed": "Failed to open external link",
    "skills.plugins.unavailable": "Unavailable",
    "skills.plugins.enabling": "Enabling",
    "skills.plugins.draining": "Draining sessions",
    "skills.plugins.cleanupWarning": "Disabled (cleanup warning)",
    "skills.plugins.unsupportedPlatform": "Unsupported on this platform",
    "skills.plugins.retryCleanup": "Retry cleanup",
    "skills.plugins.systemKind": "Vulcan Plugin",
    "skills.plugins.tools": "Tools",
    "skills.plugins.hooks": "Hooks",
    "skills.plugins.sessions": "Active sessions",
    "skills.plugins.diagnostics": "Diagnostics",
    "skills.agent.emptyTitle": "Skill catalog is not loaded",
    "skills.agent.emptyDetail": "Refresh or sync system skills to scan SKILL.md files under system and user directories.",
    "skills.agent.detailsTitle": "Agent Skill Details",
    "skills.agent.detailsDescription": "View Agent Skill details.",
  },
}
