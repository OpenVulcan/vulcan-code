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
  | "conversation.submission.creating"
  | "conversation.submission.preparing"
  | "conversation.submission.uploading"
  | "conversation.submission.uploadProgress"
  | "conversation.submission.cancelling"
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
  | "conversation.systemError.title"
  | "conversation.systemError.status"
  | "conversation.systemError.message"
  | "conversation.systemError.provider"
  | "conversation.systemError.category"
  | "conversation.systemError.stage"
  | "conversation.systemError.dispatchOutcome"
  | "conversation.systemError.semanticOutput"
  | "conversation.systemError.retryable"
  | "conversation.systemError.httpStatus"
  | "conversation.systemError.subcategory"
  | "conversation.systemError.upstreamCode"
  | "conversation.systemError.retryAfter"
  | "conversation.systemError.true"
  | "conversation.systemError.false"
  | "conversation.systemError.raw"
  | "conversation.systemError.retry"
  | "conversation.systemError.retrying"
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
  | "contextCompaction.progress.prepareRemoteTooling"
  | "contextCompaction.progress.remoteCompactionCheck"
  | "contextCompaction.progress.remoteCompactionComplete"
  | "contextCompaction.progress.remoteCompactionLocalFallback"
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
  | "composer.capability.sessionRequired"
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
  | "composer.modelPicker.currentAccount"
  | "composer.modelPicker.selectAccount"
  | "composer.aria.account"
  | "composer.accountCard.refresh"
  | "composer.accountCard.provider"
  | "composer.accountCard.serviceEntry"
  | "composer.accountCard.credentialType"
  | "composer.accountCard.protocolType"
  | "composer.accountCard.loading"
  | "composer.accountCard.metadataUnavailable"
  | "composer.accountCard.plan"
  | "composer.accountCard.accountType"
  | "composer.accountCard.nonPlan"
  | "composer.accountCard.balance"
  | "composer.accountCard.usageUnsupported"
  | "composer.accountCard.managementKeyRequired"
  | "composer.accountCard.usageNotQueried"
  | "composer.accountCard.usageUnavailable"
  | "composer.accountCard.usageInvalid"
  | "composer.accountCard.noQuotaWindows"
  | "composer.accountCard.remaining"
  | "composer.accountCard.weeklyRemaining"
  | "composer.accountCard.kimiWeeklyUsage"
  | "composer.accountCard.kimiFiveHourUsage"
  | "composer.accountCard.kimiLimit"
  | "composer.accountCard.reset"
  | "composer.accountCard.notIncluded"
  | "composer.accountCard.unlimited"
  | "composer.accountCard.tavilyPlanQuota"
  | "composer.accountCard.used"
  | "composer.accountCard.general"
  | "composer.accountCard.video"
  | "composer.accountCard.billingTokenPlan"
  | "composer.accountCard.billingPayAsYouGo"
  | "composer.accountCard.billingProviderManaged"
  | "composer.secondaryRoute.select"
  | "composer.secondaryRoute.search"
  | "composer.secondaryRoute.refresh"
  | "composer.secondaryRoute.refreshed"
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
  | "conversationActions.previewCannotRetry"
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
  | "projectSettings.codebaseMemory.availability"
  | "projectSettings.codebaseMemory.availability.ready"
  | "projectSettings.codebaseMemory.availability.degraded"
  | "projectSettings.codebaseMemory.availability.unavailable"
  | "projectSettings.codebaseMemory.availability.offline"
  | "projectSettings.codebaseMemory.availability.unknown"
  | "projectSettings.codebaseMemory.lastIndexed"
  | "projectSettings.codebaseMemory.revision"
  | "projectSettings.codebaseMemory.watcher"
  | "projectSettings.codebaseMemory.registered"
  | "projectSettings.codebaseMemory.notRegistered"
  | "projectSettings.codebaseMemory.graphSize"
  | "projectSettings.codebaseMemory.graphSizeValue"
  | "projectSettings.codebaseMemory.unavailable"
  | "projectSettings.codebaseMemory.reindex"
  | "projectSettings.codebaseMemory.refresh"
  | "projectSettings.codebaseMemory.update"
  | "projectSettings.codebaseMemory.rebuild"
  | "projectSettings.codebaseMemory.rebuildTitle"
  | "projectSettings.codebaseMemory.rebuildDescription"
  | "projectSettings.codebaseMemory.confirmRebuild"
  | "projectSettings.codebaseMemory.cancel"
  | "projectSettings.codebaseMemory.lastJob"
  | "projectSettings.codebaseMemory.job.availableDuringUpdate"
  | "projectSettings.codebaseMemory.job.mode.update"
  | "projectSettings.codebaseMemory.job.mode.rebuild"
  | "projectSettings.codebaseMemory.job.phase.queued"
  | "projectSettings.codebaseMemory.job.phase.discovering"
  | "projectSettings.codebaseMemory.job.phase.extracting"
  | "projectSettings.codebaseMemory.job.phase.resolving"
  | "projectSettings.codebaseMemory.job.phase.persisting"
  | "projectSettings.codebaseMemory.job.phase.publishing"
  | "projectSettings.codebaseMemory.job.phase.finalizing"
  | "projectSettings.codebaseMemory.job.state.queued"
  | "projectSettings.codebaseMemory.job.state.running"
  | "projectSettings.codebaseMemory.job.state.succeeded"
  | "projectSettings.codebaseMemory.job.state.failed"
  | "projectSettings.codebaseMemory.job.state.cancelled"
  | "projectSettings.codebaseMemory.enabledSaved"
  | "projectSettings.codebaseMemory.disabledSaved"
  | "projectSettings.codebaseMemory.reindexAccepted"
  | "projectSettings.codebaseMemory.updateAccepted"
  | "projectSettings.codebaseMemory.rebuildAccepted"
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
  | "vmm.experience.title"
  | "vmm.experience.subtitle"
  | "vmm.experience.quickSetup.title"
  | "vmm.experience.quickSetup.description"
  | "vmm.experience.quickSetup.action"
  | "vmm.experience.quickSetup.running"
  | "vmm.experience.status"
  | "vmm.experience.deployment"
  | "vmm.experience.setup"
  | "vmm.experience.lifecycle"
  | "vmm.experience.health"
  | "vmm.experience.prerequisites"
  | "vmm.experience.capabilities"
  | "vmm.experience.activity"
  | "vmm.experience.pendingWritebacks"
  | "vmm.experience.completedTurns"
  | "vmm.experience.lastProfileUpdate"
  | "vmm.experience.currentProject"
  | "vmm.experience.currentUser"
  | "vmm.experience.noProject"
  | "vmm.experience.noUser"
  | "vmm.experience.refresh"
  | "vmm.experience.start"
  | "vmm.experience.restart"
  | "vmm.experience.stop"
  | "vmm.experience.openAdvanced"
  | "memory.dataSpace.unknown.title"
  | "memory.dataSpace.unknown.description"
  | "memory.dataSpace.empty.title"
  | "memory.dataSpace.empty.description"
  | "memory.dataSpace.ready.title"
  | "memory.dataSpace.ready.description"
  | "memory.dataSpace.unmarkedHistory.title"
  | "memory.dataSpace.unmarkedHistory.description"
  | "memory.dataSpace.fingerprintChanged.title"
  | "memory.dataSpace.fingerprintChanged.description"
  | "memory.dataSpace.markerCorrupt.title"
  | "memory.dataSpace.markerCorrupt.description"
  | "memory.dataSpace.markerVersionUnsupported.title"
  | "memory.dataSpace.markerVersionUnsupported.description"
  | "memory.dataSpace.schemaMigrationRequired.title"
  | "memory.dataSpace.schemaMigrationRequired.description"
  | "memory.dataSpace.rebuildRunning.title"
  | "memory.dataSpace.rebuildRunning.description"
  | "memory.dataSpace.handleLater"
  | "memory.dataSpace.repair"
  | "memory.identity.title"
  | "memory.identity.description"
  | "memory.identity.refresh"
  | "memory.identity.users"
  | "memory.identity.projects"
  | "memory.identity.addUser"
  | "memory.identity.addProject"
  | "memory.identity.userName"
  | "memory.identity.teamName"
  | "memory.identity.spaceName"
  | "memory.identity.projectName"
  | "memory.identity.projectPathStructure"
  | "memory.identity.projectPathPreview"
  | "memory.identity.projectPlaceholder"
  | "memory.identity.create"
  | "memory.identity.migrate"
  | "memory.identity.delete"
  | "memory.identity.default"
  | "memory.identity.setDefault"
  | "memory.identity.protected"
  | "memory.identity.emptyUsers"
  | "memory.identity.emptyProjects"
  | "memory.identity.confirmDeleteTitle"
  | "memory.identity.confirmDeleteDescription"
  | "memory.identity.typeName"
  | "memory.identity.checking"
  | "memory.identity.migrateTitle"
  | "memory.identity.migrateDescription"
  | "memory.identity.targetProjectName"
  | "memory.common.cancel"
  | "memory.common.close"
  | "memory.common.save"
  | "memory.common.apply"
  | "memory.common.loading"
  | "memory.rules.title"
  | "memory.rules.description"
  | "memory.rules.enable"
  | "memory.rules.enableDescription"
  | "memory.rules.recall"
  | "memory.rules.recallDescription"
  | "memory.rules.organize"
  | "memory.rules.organizeDescription"
  | "memory.rules.aiTools"
  | "memory.rules.aiToolsDescription"
  | "memory.rules.recallTurns"
  | "memory.rules.profileRefresh"
  | "memory.rules.profileLimit"
  | "memory.rules.applySettings"
  | "memory.rules.applyTitle"
  | "memory.rules.applyDescription"
  | "memory.rules.allProjects"
  | "memory.rules.oneProject"
  | "memory.rules.selectProject"
  | "memory.rules.searchProject"
  | "memory.rules.noProject"
  | "memory.rules.applied"
  | "memory.content.title"
  | "memory.content.description"
  | "memory.content.sessionRequiredTitle"
  | "memory.content.sessionRequired"
  | "memory.content.search"
  | "memory.content.searchPlaceholder"
  | "memory.content.add"
  | "memory.content.empty"
  | "memory.content.noResults"
  | "memory.content.details"
  | "memory.content.category"
  | "memory.content.createdAt"
  | "memory.content.deleteTitle"
  | "memory.content.deleteDescription"
  | "memory.content.deleteReason"
  | "memory.content.addTitle"
  | "memory.content.summary"
  | "memory.content.fullDetails"
  | "memory.content.write"
  | "memory.content.sourceConversation"
  | "memory.data.title"
  | "memory.data.description"
  | "memory.data.managementReady"
  | "memory.data.managementUnavailable"
  | "memory.data.invalidResponse"
  | "memory.data.activeRuntime"
  | "memory.data.sessions"
  | "memory.data.recycleBin"
  | "memory.data.memoryEntries"
  | "memory.data.searchPlaceholder"
  | "memory.data.statusAll"
  | "memory.data.statusActive"
  | "memory.data.statusArchived"
  | "memory.data.statusRecycled"
  | "memory.data.statusForgotten"
  | "memory.data.allStatuses"
  | "memory.data.extractionPending"
  | "memory.data.extractionCompleted"
  | "memory.data.extractionFailed"
  | "memory.data.allUsers"
  | "memory.data.allProjects"
  | "memory.data.createdFrom"
  | "memory.data.createdTo"
  | "memory.data.refresh"
  | "memory.data.resetFilters"
  | "memory.data.fullTextIndex"
  | "memory.data.selected"
  | "memory.data.mixedLifecycle"
  | "memory.data.archive"
  | "memory.data.unarchive"
  | "memory.data.recycle"
  | "memory.data.restartMemory"
  | "memory.data.noSessions"
  | "memory.data.loadMore"
  | "memory.data.viewTurns"
  | "memory.data.backToSessions"
  | "memory.data.turns"
  | "memory.data.noTurns"
  | "memory.data.searchTurnPlaceholder"
  | "memory.data.details"
  | "memory.data.turnDetails"
  | "memory.data.userContent"
  | "memory.data.assistantContent"
  | "memory.data.loading"
  | "memory.data.impactTitle"
  | "memory.data.impactDescription"
  | "memory.data.confirmAction"
  | "memory.data.cancel"
  | "memory.data.turnCount"
  | "memory.data.memoryCount"
  | "memory.data.profileCount"
  | "memory.data.vectorCount"
  | "memory.data.pendingCount"
  | "memory.data.hotCount"
  | "memory.data.protectedCount"
  | "memory.data.recycleEmpty"
  | "memory.data.restore"
  | "memory.data.purge"
  | "memory.data.purgeTitle"
  | "memory.data.purgeDescription"
  | "memory.data.purgePlaceholder"
  | "memory.data.restorable"
  | "memory.data.systemBatch"
  | "memory.data.updatedAt"
  | "memory.profile.title"
  | "memory.profile.description"
  | "memory.profile.target"
  | "memory.profile.user"
  | "memory.profile.project"
  | "memory.profile.selectUser"
  | "memory.profile.selectProject"
  | "memory.profile.searchUser"
  | "memory.profile.searchProject"
  | "memory.profile.noUser"
  | "memory.profile.noProject"
  | "memory.profile.refresh"
  | "memory.profile.empty"
  | "memory.profile.instruction"
  | "memory.profile.instructionPlaceholder"
  | "memory.profile.applyInstruction"
  | "memory.profile.preview"
  | "memory.profile.previewDescription"
  | "memory.profile.priority"
  | "memory.profile.level"
  | "memory.advanced.title"
  | "memory.advanced.description"
  | "memory.advanced.deployment"
  | "memory.advanced.managed"
  | "memory.advanced.managedDescription"
  | "memory.advanced.external"
  | "memory.advanced.externalDescription"
  | "memory.advanced.connection"
  | "memory.advanced.endpoint"
  | "memory.advanced.apiKey"
  | "memory.advanced.managementEndpoint"
  | "memory.advanced.managementToken"
  | "memory.advanced.testManagement"
  | "memory.advanced.managementTimeout"
  | "memory.advanced.runtime"
  | "memory.advanced.autoStart"
  | "memory.advanced.manualStart"
  | "memory.advanced.transport"
  | "memory.advanced.diagnostics"
  | "memory.advanced.diagnosticRuntime"
  | "memory.advanced.diagnosticGrpc"
  | "memory.advanced.diagnosticController"
  | "memory.advanced.diagnosticData"
  | "memory.advanced.diagnosticLog"
  | "memory.advanced.data"
  | "memory.advanced.save"
  | "memory.advanced.backupRebuild"
  | "memory.advanced.maintenanceUnavailable"
  | "memory.maintenance.confirmTitle"
  | "memory.maintenance.confirmDescription"
  | "memory.maintenance.backupFirst"
  | "memory.maintenance.backupFirstDescription"
  | "memory.maintenance.later"
  | "memory.maintenance.progressTitle"
  | "memory.maintenance.progressDescription"
  | "memory.maintenance.progressCount"
  | "memory.maintenance.cancel"
  | "memory.maintenance.completed"
  | "memory.maintenance.failed"
  | "memory.maintenance.cancelled"
  | "memory.maintenance.startMemory"
  | "memory.maintenance.startFailed"
  | "memory.maintenance.startFailedDescription"
  | "vmm.experience.deployment.disabled"
  | "vmm.experience.deployment.managed"
  | "vmm.experience.deployment.external"
  | "vmm.experience.setup.notConfigured"
  | "vmm.experience.setup.checking"
  | "vmm.experience.setup.ready"
  | "vmm.experience.setup.incomplete"
  | "vmm.experience.setup.blocked"
  | "vmm.experience.lifecycle.stopped"
  | "vmm.experience.lifecycle.starting"
  | "vmm.experience.lifecycle.running"
  | "vmm.experience.lifecycle.stopping"
  | "vmm.experience.lifecycle.restarting"
  | "vmm.experience.lifecycle.crashed"
  | "vmm.experience.lifecycle.waitingRetry"
  | "vmm.experience.lifecycle.failed"
  | "vmm.experience.health.unknown"
  | "vmm.experience.health.healthy"
  | "vmm.experience.health.degraded"
  | "vmm.experience.health.unavailable"
  | "vmm.experience.state.available"
  | "vmm.experience.state.unavailable"
  | "vmm.experience.prerequisite.memory.title"
  | "vmm.experience.prerequisite.memory.description"
  | "vmm.experience.prerequisite.transport.title"
  | "vmm.experience.prerequisite.transport.description"
  | "vmm.experience.prerequisite.user.title"
  | "vmm.experience.prerequisite.user.description"
  | "vmm.experience.prerequisite.project.title"
  | "vmm.experience.prerequisite.project.description"
  | "vmm.experience.prerequisite.runtime.title"
  | "vmm.experience.prerequisite.runtime.description"
  | "vmm.experience.capability.recall"
  | "vmm.experience.capability.writeback"
  | "vmm.experience.capability.tools"
  | "vmm.experience.capability.profile"
  | "vmm.experience.capability.rerank"
  | "vmm.setup.title"
  | "vmm.setup.description"
  | "vmm.setup.stepLabel"
  | "vmm.setup.previous"
  | "vmm.setup.next"
  | "vmm.setup.mode.title"
  | "vmm.setup.mode.description"
  | "vmm.setup.managed.title"
  | "vmm.setup.managed.description"
  | "vmm.setup.external.title"
  | "vmm.setup.external.description"
  | "vmm.setup.inference.title"
  | "vmm.setup.inference.description"
  | "vmm.setup.inference.required"
  | "vmm.setup.inference.openCredentials"
  | "vmm.setup.scope.title"
  | "vmm.setup.scope.description"
  | "vmm.setup.scope.all_projects"
  | "vmm.setup.scope.current_project"
  | "vmm.setup.scope.currentProjectUnavailable"
  | "vmm.setup.scope.not_yet"
  | "vmm.setup.confirm.title"
  | "vmm.setup.confirm.description"
  | "vmm.setup.user"
  | "vmm.setup.user.placeholder"
  | "vmm.setup.project"
  | "vmm.setup.project.placeholder"
  | "vmm.setup.safety"
  | "vmm.setup.cancel"
  | "vmm.setup.submit"
  | "vmm.setup.running"
  | "vmm.setup.failed"
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
  | "vmm.runtime.start"
  | "vmm.runtime.restart"
  | "vmm.runtime.stop"
  | "vmm.runtime.openLog"
  | "vmm.runtime.mode"
  | "vmm.runtime.mode.managed"
  | "vmm.runtime.mode.external"
  | "vmm.runtime.phase.ready"
  | "vmm.runtime.phase.starting"
  | "vmm.runtime.phase.stopped"
  | "vmm.runtime.phase.failed"
  | "vmm.runtime.phase.external"
  | "vmm.runtime.phase.crashed"
  | "vmm.runtime.phase.waiting_retry"
  | "vmm.runtime.phase.stopping"
  | "vmm.runtime.grpcTarget"
  | "vmm.runtime.controller"
  | "vmm.runtime.controllerSpace"
  | "vmm.runtime.dataRoot"
  | "vmm.runtime.logPath"
  | "vmm.runtime.processCapability"
  | "vmm.runtime.scopeCapability"
  | "vmm.runtime.profileInstructionCapability"
  | "vmm.runtime.capabilityReady"
  | "vmm.runtime.capabilityUnavailable"
  | "vmm.result.title"
  | "vmm.result.emptySummary"
  | "vmm.result.request"
  | "vmm.result.response"
  | "vmm.result.emptyHint"
  | "vmm.result.generic"
  | "vmm.managed.behavior.identity"
  | "vmm.managed.behavior.lifecycle"
  | "vmm.managed.behavior.pipeline"
  | "vmm.managed.behavior.retention"
  | "vmm.managed.piiLanguage"
  | "vmm.managed.noiseEnabled"
  | "vmm.managed.noiseLanguage"
  | "vmm.managed.noiseSemanticEnabled"
  | "vmm.managed.noiseSemanticThreshold"
  | "vmm.managed.promptLanguage"
  | "vmm.managed.precheckIntentTimeout"
  | "vmm.managed.precheckTopK"
  | "vmm.managed.precheckSearchScope"
  | "vmm.managed.precheckSimilarityThreshold"
  | "vmm.managed.scope.team"
  | "vmm.managed.scope.space"
  | "vmm.managed.scope.project"
  | "vmm.managed.postactionInputMode"
  | "vmm.managed.inputMode.compat"
  | "vmm.managed.inputMode.strict"
  | "vmm.managed.postactionTurnThreshold"
  | "vmm.managed.postactionTokenThreshold"
  | "vmm.managed.postactionIdleTimeout"
  | "vmm.managed.postactionHistoryTurns"
  | "vmm.managed.postactionMaxInputTokens"
  | "vmm.managed.postactionMaxQueueWorkers"
  | "vmm.managed.pipelineMaxSearchKeywords"
  | "vmm.managed.pipelineMinSimilarityScore"
  | "vmm.managed.pipelineReplaceMinSimilarityScore"
  | "vmm.managed.pipelineHardDedupeThreshold"
  | "vmm.managed.pipelineHardDedupePoolTopK"
  | "vmm.managed.pipelineHybridEnabled"
  | "vmm.managed.pipelineLexicalTopK"
  | "vmm.managed.pipelineRrfK"
  | "vmm.managed.pipelineMmrEnabled"
  | "vmm.managed.pipelineMmrLambda"
  | "vmm.managed.pipelineWeibullEnabled"
  | "vmm.managed.pipelineWeibullShape"
  | "vmm.managed.pipelineWeibullScaleHours"
  | "vmm.managed.pipelineWeibullMinMultiplier"
  | "vmm.managed.pipelineWeibullReinforceWeight"
  | "vmm.managed.pipelineWeibullCrossSessionBoost"
  | "vmm.managed.retentionEnabled"
  | "vmm.managed.retentionScanInterval"
  | "vmm.managed.retentionTurnKeepExtraTurns"
  | "vmm.managed.retentionSessionIdleAfter"
  | "vmm.managed.retentionTrashDuration"
  | "vmm.managed.retentionPriorityFloor"
  | "vmm.managed.retentionMemoryLevelFloor"
  | "vmm.managed.retentionSkipProtectedShared"
  | "vmm.settings.grpcTarget"
  | "vmm.settings.apiKey"
  | "vmm.settings.managedRuntime"
  | "vmm.settings.managedBinaryPath"
  | "vmm.settings.packagedBinary"
  | "vmm.settings.autoStartPolicy"
  | "vmm.settings.autoStart"
  | "vmm.settings.manualStart"
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
  | "settings.credentialManagement"
  | "settings.providerCategory.official"
  | "settings.providerCategory.cloudMaaS"
  | "settings.providerCategory.relay"
  | "settings.providerCategory.unclassified"
  | "settings.providerDescription.default"
  | "settings.providerDescription.minimax"
  | "settings.providerDescription.kimi"
  | "settings.providerDescription.deepseek"
  | "settings.providerDescription.alibaba"
  | "settings.providerDescription.openrouter"
  | "settings.providerDescription.apikeyfun"
  | "settings.providerDescription.claudeapi"
  | "settings.providerDescription.qiniu"
  | "settings.providerDescription.fennoai"
  | "settings.providerDescription.code0"
  | "settings.providerDescription.tavily"
  | "settings.providerDescription.openai"
  | "settings.providerDescription.anthropic"
  | "settings.providerDescription.google"
  | "settings.providerSummary.serviceEntries"
  | "settings.providerSummary.protocols"
  | "settings.providerSummary.serviceEntriesTitle"
  | "settings.providerSummary.protocolsTitle"
  | "settings.serviceEntry.title.codingPlan"
  | "settings.serviceEntry.title.tokenPlan"
  | "settings.serviceEntry.title.modelStudio"
  | "settings.serviceEntry.tag.personal"
  | "settings.serviceEntry.tag.team"
  | "settings.serviceEntry.tag.china"
  | "settings.serviceEntry.tag.international"
  | "settings.providerOnboarding.notConfigured"
  | "settings.providerOnboarding.description"
  | "settings.providerOnboarding.serviceEntries"
  | "settings.providerOnboarding.register"
  | "settings.providerOnboarding.referralSignup"
  | "settings.providerOnboarding.addCredential"
  | "settings.providerConnection.serviceOrigin"
  | "settings.providerConnection.serviceOriginDescription"
  | "settings.providerOnboarding.filteredEmpty"
  | "settings.providerOnboarding.filteredEmptyDescription"
  | "settings.providerOnboarding.externalConfirmation"
  | "settings.providerOnboarding.externalOpenAnyway"
  | "settings.providerOnboarding.externalOpenFailed"
  | "settings.providerPromotion.neutralDisclosure"
  | "settings.providerPromotion.referralDisclosure"
  | "settings.providerPromotion.sponsoredDisclosure"
  | "settings.modelList"
  | "settings.providerRetry"
  | "settings.aiVendors"
  | "settings.aiWebSearch"
  | "settings.aiWebExtract"
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
  | "settings.credentialManagement.description"
  | "settings.modelList.description"
  | "settings.providerRetry.description"
  | "settings.aiVendors.description"
  | "settings.aiWebSearch.description"
  | "settings.aiWebExtract.description"
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
  | "settings.modelOverrides.title"
  | "settings.modelOverrides.description"
  | "settings.modelOverrides.limits"
  | "settings.modelOverrides.limitsDescription"
  | "settings.modelOverrides.contextLimit"
  | "settings.modelOverrides.outputLimit"
  | "settings.modelOverrides.leaveBlank"
  | "settings.modelOverrides.providerMaximum"
  | "settings.modelOverrides.providerNotDeclared"
  | "settings.modelOverrides.cache"
  | "settings.modelOverrides.cacheDescription"
  | "settings.modelOverrides.cacheMode"
  | "settings.modelOverrides.providerDefault"
  | "settings.modelOverrides.managedExplicit"
  | "settings.modelOverrides.cacheTtl"
  | "settings.modelOverrides.parameters"
  | "settings.modelOverrides.parametersDescription"
  | "settings.modelOverrides.noParameters"
  | "settings.modelOverrides.unsupportedParameter"
  | "settings.modelOverrides.enabled"
  | "settings.customModels.title"
  | "settings.customModels.description"
  | "settings.customModels.pageModels"
  | "settings.customModels.pageProviderSettings"
  | "settings.customModels.search"
  | "settings.customModels.filterAll"
  | "settings.customModels.filterEnabled"
  | "settings.customModels.filterDisabled"
  | "settings.customModels.importDiscovered"
  | "settings.customModels.importTitle"
  | "settings.customModels.importDescription"
  | "settings.customModels.importSelected"
  | "settings.customModels.addManual"
  | "settings.customModels.enableAll"
  | "settings.customModels.disableAll"
  | "settings.customModels.enabled"
  | "settings.customModels.disabled"
  | "settings.credentialModels.manage"
  | "settings.credentialModels.title"
  | "settings.credentialModels.description"
  | "settings.credentialModels.search"
  | "settings.credentialModels.filterAll"
  | "settings.credentialModels.filterEnabled"
  | "settings.credentialModels.filterDisabled"
  | "settings.credentialModels.enableAll"
  | "settings.credentialModels.disableAll"
  | "settings.credentialModels.sync"
  | "settings.credentialModels.syncing"
  | "settings.credentialModels.count"
  | "settings.credentialModels.empty"
  | "settings.credentialModels.noResults"
  | "settings.credentialModels.directoryFixed"
  | "settings.credentialModels.directoryDynamic"
  | "settings.credentialModels.directoryManual"
  | "settings.credentialModels.directoryUnavailable"
  | "settings.credentialModels.disableConfirmTitle"
  | "settings.credentialModels.disableConfirmDescription"
  | "settings.credentialModels.cancel"
  | "settings.credentialModels.confirmDisable"
  | "settings.credentialModels.futureEnabled"
  | "settings.credentialModels.futureDisabled"
  | "settings.customModels.empty"
  | "settings.customModels.noModelsToImport"
  | "settings.customModels.removeConfirm"
  | "settings.customProviders.removeTitle"
  | "settings.customProviders.removeDescription"
  | "settings.customProviders.removeModelsWarning"
  | "settings.customProviders.removeModelsWarningGeneric"
  | "settings.customProviders.removeCredentialsWarning"
  | "settings.customProviders.removeSessionsWarning"
  | "settings.customProviders.modelCount"
  | "settings.customProviders.manageModels"
  | "settings.customProviders.manageModelsFor"
  | "settings.customProviders.addTitle"
  | "settings.customProviders.editTitle"
  | "settings.customProviders.dialogDescription"
  | "settings.customProviders.progressLabel"
  | "settings.customProviders.step.connection"
  | "settings.customProviders.step.credential"
  | "settings.customProviders.step.result"
  | "settings.customProviders.displayName"
  | "settings.customProviders.baseUrl"
  | "settings.customProviders.protocol"
  | "settings.customProviders.codexCapabilities"
  | "settings.customProviders.codexWebsocket"
  | "settings.customProviders.codexWebsocketHint"
  | "settings.customProviders.codexRemoteCompaction"
  | "settings.customProviders.codexRemoteCompactionHint"
  | "settings.customProviders.authentication"
  | "settings.customProviders.discoveryMode"
  | "settings.customProviders.discoveryStandard"
  | "settings.customProviders.discoveryCustomPath"
  | "settings.customProviders.discoveryManual"
  | "settings.customProviders.discoveryPath"
  | "settings.customProviders.protocolEndpoint"
  | "settings.customProviders.modelsEndpoint"
  | "settings.customProviders.manualOnly"
  | "settings.customProviders.validationTitle"
  | "settings.customProviders.validationEditDescription"
  | "settings.customProviders.validationCreateDescription"
  | "settings.customProviders.validationCredentialName"
  | "settings.customProviders.validationCredentialRequired"
  | "settings.customProviders.credentialFreeEditTitle"
  | "settings.customProviders.credentialFreeEditDescription"
  | "settings.customProviders.firstCredentialName"
  | "settings.customProviders.apiKey"
  | "settings.customProviders.apiKeyEditPlaceholder"
  | "settings.customProviders.apiKeyCreatePlaceholder"
  | "settings.customProviders.credentialRequired"
  | "settings.customProviders.back"
  | "settings.customProviders.next"
  | "settings.customProviders.probing"
  | "settings.customProviders.probe"
  | "settings.customProviders.validationResultDescription"
  | "settings.customProviders.validationModelCount"
  | "settings.customProviders.validationPageCount"
  | "settings.customProviders.validationCredential"
  | "settings.customProviders.validationObservedAt"
  | "settings.customProviders.validationModelsTitle"
  | "settings.customProviders.validationEvidence"
  | "settings.customProviders.validationModelsHint"
  | "settings.customProviders.manualFallbackHint"
  | "settings.customProviders.pendingSaveHint"
  | "settings.customProviders.saving"
  | "settings.customProviders.saveVerified"
  | "settings.customProviders.saveManual"
  | "settings.customProviders.savePending"
  | "settings.customProviders.pendingConfirmTitle"
  | "settings.customProviders.pendingConfirmDescription"
  | "settings.customProviders.confirmPending"
  | "settings.customProviders.protocolOverridesConfirmTitle"
  | "settings.customProviders.protocolOverridesConfirmDescription"
  | "settings.customProviders.protocolOverridesConfirmAction"
  | "settings.providerModels.description"
  | "settings.providerModels.count"
  | "settings.providerModels.empty"
  | "settings.customProviders.displayNameRequired"
  | "settings.customProviders.baseUrlInvalid"
  | "settings.customProviders.discoveryPathInvalid"
  | "settings.customProviders.discovery.standard"
  | "settings.customProviders.discovery.custom_path"
  | "settings.customProviders.discovery.manual"
  | "settings.customModels.identityRequired"
  | "settings.customModels.duplicateId"
  | "settings.customModels.parametersInvalid"
  | "settings.customModels.basic"
  | "settings.customModels.basicHint"
  | "settings.customModels.modelId"
  | "settings.customModels.displayName"
  | "settings.customModels.contextLimit"
  | "settings.customModels.outputLimit"
  | "settings.customModels.modalities"
  | "settings.customModels.modalitiesHint"
  | "settings.customModels.input"
  | "settings.customModels.output"
  | "settings.customModels.behavior"
  | "settings.customModels.behaviorHint"
  | "settings.customModels.toolCalling"
  | "settings.customModels.parallelTools"
  | "settings.customModels.reasoning"
  | "settings.customModels.reasoningControl"
  | "settings.customModels.supportUnknown"
  | "settings.customModels.supportNative"
  | "settings.customModels.supportEquivalent"
  | "settings.customModels.supportLossy"
  | "settings.customModels.supportUnsupported"
  | "settings.customModels.controlUnknown"
  | "settings.customModels.controlFixedOff"
  | "settings.customModels.controlFixedOn"
  | "settings.customModels.controlToggle"
  | "settings.customModels.controlEffortSelection"
  | "settings.customModels.controlProviderAdaptive"
  | "settings.customModels.reasoningEfforts"
  | "settings.customModels.effortNone"
  | "settings.customModels.effortAdaptive"
  | "settings.customModels.effortMinimal"
  | "settings.customModels.effortLow"
  | "settings.customModels.effortMedium"
  | "settings.customModels.effortHigh"
  | "settings.customModels.effortExtraHigh"
  | "settings.customModels.effortMax"
  | "settings.customModels.effortUltra"
  | "settings.customModels.advanced"
  | "settings.customModels.advancedHint"
  | "settings.customModels.parameters"
  | "settings.customModels.projection"
  | "settings.customModels.projectionInvalid"
  | "settings.customModels.remove"
  | "settings.customModels.selectHint"
  | "settings.customModels.save"
  | "settings.customModels.defaultsTitle"
  | "settings.customModels.defaultsDescription"
  | "settings.customModels.defaultsEdit"
  | "settings.customModels.contextSummary"
  | "settings.customModels.outputSummary"
  | "settings.customModels.textInputOutput"
  | "settings.customModels.outputRecommendation"
  | "settings.customModels.defaultsContextInvalid"
  | "settings.customModels.defaultsOutputInvalid"
  | "settings.customModels.defaultsReasoningInvalid"
  | "settings.customModels.defaultsCustomSource"
  | "settings.customModels.defaultsSystemSource"
  | "settings.customModels.defaultsSave"
  | "settings.customModels.fixedTextOutput"
  | "settings.customModels.fixedTextOutputDescription"
  | "settings.customModels.toolCallingDefaultDescription"
  | "settings.customModels.templateSearching"
  | "settings.customModels.templateUpdateAvailable"
  | "settings.customModels.templateUpdateDescription"
  | "settings.customModels.templateCurrent"
  | "settings.customModels.templateLatest"
  | "settings.customModels.templateApplyUpdate"
  | "settings.customModels.templateApplied"
  | "settings.customModels.templateRestore"
  | "settings.customModels.templateNone"
  | "settings.customModels.templateDefaultFallback"
  | "settings.customModels.templateRejected"
  | "settings.customModels.templateReviewAgain"
  | "settings.customModels.templateSuggested"
  | "settings.customModels.templateApply"
  | "settings.customModels.templateKeepDefaults"
  | "settings.customModels.templateMatch.exact"
  | "settings.customModels.templateMatch.alias"
  | "settings.customModels.templateMatch.provider_agnostic"
  | "settings.customModels.discoveryTitle"
  | "settings.customModels.discoverySummary"
  | "settings.customModels.reviewTemplates"
  | "settings.customModels.reviewTemplatesTitle"
  | "settings.customModels.reviewTemplatesDescription"
  | "settings.customModels.reviewExact"
  | "settings.customModels.reviewMissing"
  | "settings.customModels.reviewExactDescription"
  | "settings.customModels.reviewMissingDescription"
  | "settings.customModels.reviewComplete"
  | "settings.customModels.applyAllExact"
  | "settings.customModels.refreshAll"
  | "settings.customModels.refreshAccount"
  | "settings.customModels.lastSynchronized"
  | "settings.customModels.notSynchronized"
  | "settings.customModels.discoveryDetails"
  | "settings.customModels.discoveryModelCount"
  | "settings.customModels.discoveryPageCount"
  | "settings.customModels.discoverySchema"
  | "settings.customModels.discovery.notChecked"
  | "settings.customModels.discovery.ready"
  | "settings.customModels.discovery.empty"
  | "settings.customModels.discovery.unsupported"
  | "settings.customModels.discovery.authenticationFailed"
  | "settings.customModels.discovery.rateLimited"
  | "settings.customModels.discovery.temporarilyUnavailable"
  | "settings.customModels.discovery.invalidResponse"
  | "settings.customModels.discovery.stale"
  | "settings.customModels.discovery.failed"
  | "settings.customModels.sourceApi"
  | "settings.customModels.sourceManual"
  | "settings.customModels.sourceUserOverride"
  | "settings.customModels.sourceAccount"
  | "settings.customModels.sourceTemplate"
  | "settings.customModels.sourceProviderDefault"
  | "settings.customModels.backToList"
  | "settings.customModels.tabOverview"
  | "settings.customModels.tabAccounts"
  | "settings.customModels.tabCapabilities"
  | "settings.customModels.tabAdvanced"
  | "settings.customModels.accountReturned"
  | "settings.customModels.accountNotReturned"
  | "settings.customModels.accountAvailable"
  | "settings.customModels.accountUnavailable"
  | "settings.customModels.accountInherit"
  | "settings.customModels.accountForceEnable"
  | "settings.customModels.accountForceDisable"
  | "settings.customModels.capabilityAccount"
  | "settings.customModels.viewAccountCapabilities"
  | "settings.customModels.pendingValidation"
  | "settings.customModels.pendingCannotDefault"
  | "settings.customModels.validateModel"
  | "settings.customModels.validateModelTitle"
  | "settings.customModels.validateModelDescription"
  | "settings.customModels.confirmValidation"
  | "settings.customModels.validationComplete"
  | "settings.customModels.validationSucceeded"
  | "settings.customModels.noAccounts"
  | "settings.customModels.noAccountsDescription"
  | "settings.customModels.saveCapabilityOverrides"
  | "settings.customModels.changeSummaryTitle"
  | "settings.customModels.changeSummaryDescription"
  | "settings.customModels.confirmChanges"
  | "settings.customModels.summaryModel"
  | "settings.customModels.summaryAccount"
  | "settings.customModels.summaryAvailability"
  | "settings.customModels.summaryRoutingState"
  | "settings.customModels.summaryCapabilityOverrides"
  | "settings.customModels.summaryRemoveOverrides"
  | "settings.customModels.summaryTemplate"
  | "settings.customModels.summaryDefaults"
  | "settings.customModels.summaryNotEntered"
  | "settings.customModels.advancedChangeSummaryDescription"
  | "settings.customModels.summaryAdvancedDefinitions"
  | "settings.customModels.summaryCurrentModel"
  | "settings.customModels.textOnly"
  | "settings.customModels.advancedUnavailable"
  | "settings.customModels.advancedCreateHint"
  | "settings.customModels.advancedCreate"
  | "settings.customModels.saveAdvanced"
  | "settings.customModels.unsavedAdvanced"
  | "settings.customModels.modelIdRequired"
  | "settings.customModels.manualDescription"
  | "settings.customModels.templateChoice"
  | "settings.customModels.discardTitle"
  | "settings.customModels.discardDescription"
  | "settings.customModels.discard"
  | "settings.modelCapabilities.title"
  | "settings.modelCapabilities.shortLabel"
  | "settings.modelCapabilities.overlayHint"
  | "settings.modelCapabilities.contextLimit"
  | "settings.modelCapabilities.overrideContext"
  | "settings.modelCapabilities.contextHint"
  | "settings.modelCapabilities.contextInvalid"
  | "settings.modelCapabilities.inputResources"
| "settings.modelCapabilities.outputResources"
| "settings.modelCapabilities.overrideResources"
| "settings.modelCapabilities.restore"
  | "settings.modelCapabilities.modelSource"
  | "settings.modelCapabilities.inheritedSource"
  | "settings.contextWindowPolicy.title"
  | "settings.contextWindowPolicy.description"
  | "settings.contextWindowPolicy.effectivePercent"
  | "settings.contextWindowPolicy.autoCompactPercent"
  | "settings.contextWindowPolicy.autoCompactTokenLimit"
  | "settings.contextWindowPolicy.scope"
  | "settings.contextWindowPolicy.scopeInherit"
  | "settings.contextWindowPolicy.scopeTotal"
  | "settings.contextWindowPolicy.scopeBodyAfterPrefix"
  | "settings.contextWindowPolicy.fallbackBuffer"
  | "settings.contextWindowPolicy.clear"
  | "settings.models.enabled"
  | "settings.models.disabled"
  | "settings.models.toggle"
  | "settings.models.loadingCatalog"
  | "settings.models.provider"
  | "settings.models.liveModels"
  | "settings.models.noProviderSelected"
  | "settings.models.catalogSummary"
  | "settings.models.memoryOnly"
  | "settings.models.emptyCatalog"
  | "settings.models.availableAccounts"
  | "settings.models.availableCredentials"
  | "settings.models.status"
  | "settings.models.additionalConditions"
  | "settings.models.collapseProviders"
  | "settings.models.expandProviders"
  | "settings.models.searchLabel"
  | "settings.models.searchPlaceholder"
  | "settings.models.statusFilter"
  | "settings.models.filterAll"
  | "settings.models.filterEnabled"
  | "settings.models.filterDisabled"
  | "settings.models.enableAll"
  | "settings.models.disableAll"
  | "settings.models.filteredSummary"
  | "settings.models.emptyFilter"
  | "settings.models.inputModalities"
  | "settings.models.outputModalities"
  | "settings.models.modality.text"
  | "settings.models.modality.image"
  | "settings.models.modality.audio"
  | "settings.models.modality.video"
  | "settings.models.modality.file"
  | "settings.models.modality.embedding"
  | "settings.models.modality.rerank"
  | "settings.inference.title"
  | "settings.inference.subtitle"
  | "settings.inference.vmmReasoningNotice"
  | "settings.inference.profileVmm"
  | "settings.inference.llm"
  | "settings.inference.embedding"
  | "settings.inference.rerank"
  | "settings.inference.noRoute"
  | "settings.inference.modelRegion"
  | "settings.inference.accountRegion"
  | "settings.inference.selectModel"
  | "settings.inference.selectAccount"
  | "settings.inference.selectModelFirst"
  | "settings.inference.accountFilter"
  | "settings.inference.noAccountMatches"
  | "settings.inference.unavailable"
  | "settings.inference.unavailableReason"
  | "settings.inference.testConnection"
  | "settings.inference.testEmbeddingSuccess"
  | "settings.inference.testRerankSuccess"
  | "settings.inference.credentialAvailable"
  | "settings.inference.catalogGeneration"
  | "settings.inference.capabilityRevision"
  | "settings.inference.scopeInteractive"
  | "settings.inference.scopeBackground"
  | "settings.inference.scopeExternal"
  | "settings.inference.internalToggle"
  | "settings.inference.configureDefaults"
  | "settings.inference.overridesTitle"
  | "settings.inference.overridesDescription"
  | "settings.inference.inheritModelDefault"
  | "settings.inference.reasoningEffort"
  | "settings.inference.embeddingTask"
  | "settings.inference.embeddingDimensions"
  | "settings.inference.embeddingOutputKind"
  | "settings.inference.embeddingEncoding"
  | "settings.inference.rerankTopN"
  | "settings.inference.rerankTruncation"
  | "settings.inference.rerankReturnContent"
  | "settings.inference.rerankRestartTitle"
  | "settings.inference.rerankRestartDescription"
  | "settings.inference.rerankRestartCancel"
  | "settings.inference.rerankRestartConfirm"
  | "settings.inference.rerankRestartFailed"
  | "settings.inference.maintenanceLocked"
  | "settings.inference.purpose.precheckL1"
| "settings.inference.purpose.precheckL1Description"
| "settings.inference.purpose.precheckL2"
| "settings.inference.purpose.precheckL2Description"
| "settings.inference.purpose.postactionL1"
| "settings.inference.purpose.postactionL1Description"
| "settings.inference.purpose.postactionL2"
| "settings.inference.purpose.postactionL2Description"
| "settings.inference.purpose.profileInstruction"
| "settings.inference.purpose.profileInstructionDescription"
| "settings.inference.purpose.reserve"
| "settings.inference.purpose.reserveDescription"
  | "settings.providerTools.title"
  | "settings.providerTools.description"
  | "settings.providerTools.select"
  | "settings.providerTools.disabled"
  | "settings.providerTools.unavailable"
  | "settings.providerTools.imageGenerate"
  | "settings.providerTools.imageEdit"
  | "settings.providerTools.mediaAnalyze"
  | "settings.providerTools.musicGenerate"
  | "settings.providerTools.musicCover"
  | "settings.providerTools.speechSynthesize"
  | "settings.providerTools.speechTranscribe"
  | "settings.providerTools.videoGenerate"
  | "settings.providerTools.videoEdit"
  | "settings.providerTools.webSearch"
  | "settings.providerTools.webExtract"
  | "settings.providerTools.loading"
  | "settings.providerTools.enabled"
  | "settings.providerTools.default"
  | "settings.providerTools.setDefault"
  | "settings.providerTools.noTargets"
  | "settings.providerTools.change"
  | "settings.providerTools.noSelection"
  | "settings.providerTools.configure"
  | "settings.providerTools.dialogTitle"
  | "settings.providerTools.dialogDescription"
  | "settings.providerTools.providers"
  | "settings.providerTools.targets"
  | "settings.providerTools.systemDefault"
  | "settings.providerTools.systemDefaultHint"
  | "settings.providerTools.model"
  | "settings.providerTools.account"
  | "settings.providerTools.selectModel"
  | "settings.providerTools.selectAccount"
  | "composer.capability.conflict.tooltip"
  | "composer.capability.conflict.banner"
  | "composer.capability.conflict.missingSelection"
  | "composer.capability.conflict.targetUnavailable"
  | "settings.providerTools.disable"
  | "settings.providerTools.cancel"
  | "settings.providerTools.save"
  | "settings.providerTools.test"
  | "settings.providerTools.testSearchTitle"
  | "settings.providerTools.testExtractTitle"
  | "settings.providerTools.testSearchDescription"
  | "settings.providerTools.testExtractDescription"
  | "settings.providerTools.testQuery"
  | "settings.providerTools.testQueryPlaceholder"
  | "settings.providerTools.testMaxResults"
  | "settings.providerTools.testIncludeDomains"
  | "settings.providerTools.testExcludeDomains"
  | "settings.providerTools.testDomainsPlaceholder"
  | "settings.providerTools.testUrls"
  | "settings.providerTools.testUrlsPlaceholder"
  | "settings.providerTools.testExtractQuery"
  | "settings.providerTools.testChunksPerSource"
  | "settings.providerTools.testDepth"
  | "settings.providerTools.testDepthBasic"
  | "settings.providerTools.testDepthAdvanced"
  | "settings.providerTools.testFormat"
  | "settings.providerTools.testFormatMarkdown"
  | "settings.providerTools.testFormatText"
  | "settings.providerTools.testTimeoutSeconds"
  | "settings.providerTools.testIncludeImages"
  | "settings.providerTools.testIncludeFavicon"
  | "settings.providerTools.testConsumesQuota"
  | "settings.providerTools.testRunning"
  | "settings.providerTools.testResults"
  | "settings.providerTools.testNoResults"
  | "settings.providerTools.testSucceeded"
  | "settings.providerTools.testFailed"
  | "settings.providerTools.testProviderRequestId"
  | "toolPresentation.providerText.title"
  | "toolPresentation.providerText.empty"
  | "toolPresentation.providerSearch.title"
  | "toolPresentation.providerSearch.results"
  | "toolPresentation.providerSearch.empty"
  | "toolPresentation.providerExtract.title"
  | "toolPresentation.providerExtract.results"
  | "toolPresentation.providerExtract.failures"
  | "toolPresentation.providerExtract.images"
  | "toolPresentation.providerExtract.empty"
  | "toolPresentation.providerStructured.embedding"
  | "toolPresentation.providerStructured.rerank"
  | "toolPresentation.providerStructured.composite"
  | "toolPresentation.providerStructured.items"
  | "settings.modelOverrides.disabled"
  | "settings.modelOverrides.cancel"
  | "settings.modelOverrides.save"
  | "settings.modelOverrides.saving"
  | "settings.modelOverrides.validation.safeInteger"
  | "settings.modelOverrides.validation.positiveInteger"
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
  | "gitCommitDialog.reviewSettings"
| "gitCommitDialog.monitoringEnabled"
| "gitCommitDialog.monitoringDisabled"
| "gitCommitDialog.monitoringManager"
| "gitCommitDialog.monitoringManagerDescription"
| "gitCommitDialog.moveRepositoryUp"
| "gitCommitDialog.moveRepositoryDown"
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
    "conversation.submission.creating": "正在创建会话",
    "conversation.submission.preparing": "正在准备工作区",
    "conversation.submission.uploading": "正在上传媒体",
    "conversation.submission.uploadProgress": "媒体上传进度",
    "conversation.submission.cancelling": "正在取消上传",
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
    "conversation.systemError.title": "系统错误",
    "conversation.systemError.status": "失败",
    "conversation.systemError.message": "错误消息",
    "conversation.systemError.provider": "Provider",
    "conversation.systemError.category": "错误类别",
    "conversation.systemError.stage": "执行阶段",
    "conversation.systemError.dispatchOutcome": "请求派发",
    "conversation.systemError.semanticOutput": "已观察到语义输出",
    "conversation.systemError.retryable": "允许重试",
    "conversation.systemError.httpStatus": "HTTP 状态",
    "conversation.systemError.subcategory": "错误子类别",
    "conversation.systemError.upstreamCode": "上游错误码",
    "conversation.systemError.retryAfter": "重试等待",
    "conversation.systemError.true": "是",
    "conversation.systemError.false": "否",
    "conversation.systemError.raw": "查看原始错误",
    "conversation.systemError.retry": "重试",
    "conversation.systemError.retrying": "重试中",
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
    "contextCompaction.progress.prepareRemoteTooling": "正在按当前会话重建远端压缩所需的工具、插件与 MCP 快照。",
    "contextCompaction.progress.remoteCompactionCheck": "正在准备远端压缩请求并等待供应商响应；耗时取决于上游，但当前任务仍在运行。",
    "contextCompaction.progress.remoteCompactionComplete": "供应商远端压缩已完成，正在应用新的上下文检查点。",
    "contextCompaction.progress.remoteCompactionLocalFallback": "远端压缩未执行或未安全完成，正在改用本地压缩；当前任务仍在运行。",
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
    "composer.capability.dialogDescription": "每项能力只选择一个目标；点击保存后写入当前会话，并从下一次对话开始生效。",
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
    "composer.capability.sessionRequired": "发送首条消息创建会话后，可保存会话级能力选择。",
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
    "composer.modelPicker.filter": "输入模型名称或 ID 过滤",
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
    "composer.modelPicker.currentAccount": "当前账号：{account}",
    "composer.modelPicker.selectAccount": "为 {model} 选择账号",
    "composer.aria.account": "选择模型账号",
    "composer.accountCard.refresh": "刷新账号用量",
    "composer.accountCard.provider": "来源供应商",
    "composer.accountCard.serviceEntry": "服务入口",
    "composer.accountCard.credentialType": "凭据类型",
    "composer.accountCard.protocolType": "协议类型",
    "composer.accountCard.loading": "正在读取账号信息...",
    "composer.accountCard.metadataUnavailable": "账号信息暂不可用",
    "composer.accountCard.plan": "会员套餐",
    "composer.accountCard.accountType": "账号类型",
    "composer.accountCard.nonPlan": "非套餐",
    "composer.accountCard.balance": "余额",
    "composer.accountCard.usageUnsupported": "不支持用量查询",
    "composer.accountCard.managementKeyRequired": "添加可选管理密钥后可查询余额",
    "composer.accountCard.usageNotQueried": "用量尚未查询",
    "composer.accountCard.usageUnavailable": "账号用量暂时不可用",
    "composer.accountCard.usageInvalid": "账号用量响应格式无效，请重新刷新",
    "composer.accountCard.noQuotaWindows": "该供应商不提供套餐额度窗口",
    "composer.accountCard.remaining": "剩余",
    "composer.accountCard.weeklyRemaining": "周剩余",
    "composer.accountCard.kimiWeeklyUsage": "周用量",
    "composer.accountCard.kimiFiveHourUsage": "5 小时用量",
    "composer.accountCard.kimiLimit": "额度",
    "composer.accountCard.reset": "重置",
    "composer.accountCard.notIncluded": "不包含",
    "composer.accountCard.unlimited": "无限",
    "composer.accountCard.tavilyPlanQuota": "套餐",
    "composer.accountCard.used": "已用",
    "composer.accountCard.general": "通用",
    "composer.accountCard.video": "视频",
    "composer.accountCard.billingTokenPlan": "套餐",
    "composer.accountCard.billingPayAsYouGo": "按量计费",
    "composer.accountCard.billingProviderManaged": "供应商管理",
    "composer.secondaryRoute.select": "选择二级供应商",
    "composer.secondaryRoute.search": "搜索二级供应商",
    "composer.secondaryRoute.refresh": "刷新二级供应商",
    "composer.secondaryRoute.refreshed": "二级供应商列表已刷新。",
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
    "conversationActions.previewCannotRetry": "浏览器预览模式下无法重试真实会话。",
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
    "projectSettings.codebaseMemory.availability": "已发布索引",
    "projectSettings.codebaseMemory.availability.ready": "可用",
    "projectSettings.codebaseMemory.availability.degraded": "降级可用",
    "projectSettings.codebaseMemory.availability.unavailable": "尚不可用",
    "projectSettings.codebaseMemory.availability.offline": "离线缓存",
    "projectSettings.codebaseMemory.availability.unknown": "尚未检查",
    "projectSettings.codebaseMemory.lastIndexed": "最近成功索引",
    "projectSettings.codebaseMemory.revision": "索引修订",
    "projectSettings.codebaseMemory.watcher": "增量监视",
    "projectSettings.codebaseMemory.registered": "已注册",
    "projectSettings.codebaseMemory.notRegistered": "未注册",
    "projectSettings.codebaseMemory.graphSize": "当前图谱",
    "projectSettings.codebaseMemory.graphSizeValue": "{nodes} 个节点 · {edges} 条边",
    "projectSettings.codebaseMemory.unavailable": "暂不可用",
    "projectSettings.codebaseMemory.reindex": "重建项目索引",
    "projectSettings.codebaseMemory.refresh": "刷新索引状态",
    "projectSettings.codebaseMemory.update": "同步索引",
    "projectSettings.codebaseMemory.rebuild": "完整重建",
    "projectSettings.codebaseMemory.rebuildTitle": "完整重建项目索引？",
    "projectSettings.codebaseMemory.rebuildDescription": "完整重建会在隔离数据库中重新扫描项目，成功后再原子替换现有索引。重建期间当前可用索引仍可继续查询。",
    "projectSettings.codebaseMemory.confirmRebuild": "确认完整重建",
    "projectSettings.codebaseMemory.cancel": "取消",
    "projectSettings.codebaseMemory.lastJob": "最近任务",
    "projectSettings.codebaseMemory.job.availableDuringUpdate": "后台任务不会让已经发布的索引进入不可用状态。",
    "projectSettings.codebaseMemory.job.mode.update": "同步索引",
    "projectSettings.codebaseMemory.job.mode.rebuild": "完整重建",
    "projectSettings.codebaseMemory.job.phase.queued": "等待调度",
    "projectSettings.codebaseMemory.job.phase.discovering": "扫描文件",
    "projectSettings.codebaseMemory.job.phase.extracting": "提取结构",
    "projectSettings.codebaseMemory.job.phase.resolving": "解析关系",
    "projectSettings.codebaseMemory.job.phase.persisting": "写入索引",
    "projectSettings.codebaseMemory.job.phase.publishing": "发布索引",
    "projectSettings.codebaseMemory.job.phase.finalizing": "完成收尾",
    "projectSettings.codebaseMemory.job.state.queued": "等待中",
    "projectSettings.codebaseMemory.job.state.running": "运行中",
    "projectSettings.codebaseMemory.job.state.succeeded": "已完成",
    "projectSettings.codebaseMemory.job.state.failed": "失败",
    "projectSettings.codebaseMemory.job.state.cancelled": "已取消",
    "projectSettings.codebaseMemory.enabledSaved": "已启用项目 Codebase Memory。",
    "projectSettings.codebaseMemory.disabledSaved": "已停用项目 Codebase Memory，已有索引仍保留。",
    "projectSettings.codebaseMemory.reindexAccepted": "项目索引重建请求已提交。",
    "projectSettings.codebaseMemory.updateAccepted": "索引同步任务已提交。",
    "projectSettings.codebaseMemory.rebuildAccepted": "完整重建任务已提交。",
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
    "vmm.experience.title": "记忆服务",
    "vmm.experience.subtitle": "配置、启动并管理 Vulcan Memory Mesh 长期记忆。",
    "vmm.experience.quickSetup.title": "尚未启用长期记忆",
    "vmm.experience.quickSetup.description": "使用推荐设置启用本机托管记忆。Vulcan Code 会检查运行文件、模型配置和数据安全状态后再启动。",
    "vmm.experience.quickSetup.action": "开始快速配置",
    "vmm.experience.quickSetup.running": "正在保存配置并启动记忆服务…",
    "vmm.experience.status": "服务状态",
    "vmm.experience.deployment": "运行方式",
    "vmm.experience.setup": "配置状态",
    "vmm.experience.lifecycle": "进程状态",
    "vmm.experience.health": "可用状态",
    "vmm.experience.prerequisites": "配置检查",
    "vmm.experience.capabilities": "可用能力",
    "vmm.experience.activity": "近期活动",
    "vmm.experience.pendingWritebacks": "待写入记忆",
    "vmm.experience.completedTurns": "已处理对话",
    "vmm.experience.lastProfileUpdate": "最近画像更新",
    "vmm.experience.currentProject": "默认记忆项目",
    "vmm.experience.currentUser": "默认记忆用户",
    "vmm.experience.noProject": "尚未选择默认记忆项目",
    "vmm.experience.noUser": "尚未选择默认记忆用户",
    "vmm.experience.refresh": "刷新状态",
    "vmm.experience.start": "启动服务",
    "vmm.experience.restart": "重新启动",
    "vmm.experience.stop": "停止服务",
    "vmm.experience.openAdvanced": "打开高级设置",
    "memory.dataSpace.unknown.title": "尚未检查记忆数据",
    "memory.dataSpace.unknown.description": "启动本地记忆服务后会自动检查数据兼容性。",
    "memory.dataSpace.empty.title": "记忆数据空间为空",
    "memory.dataSpace.empty.description": "首次启动时会安全创建新的记忆数据空间。",
    "memory.dataSpace.ready.title": "记忆数据可用",
    "memory.dataSpace.ready.description": "现有记忆数据与当前向量模型配置一致。",
    "memory.dataSpace.unmarkedHistory.title": "检测到历史记忆数据",
    "memory.dataSpace.unmarkedHistory.description": "现有数据缺少可信的向量空间标记。为避免错误解释历史向量，需要先备份并重建索引。",
    "memory.dataSpace.fingerprintChanged.title": "向量模型配置已变化",
    "memory.dataSpace.fingerprintChanged.description": "历史向量与当前模型或维度不一致，需要备份并重建索引后才能继续。",
    "memory.dataSpace.markerCorrupt.title": "记忆数据标记损坏",
    "memory.dataSpace.markerCorrupt.description": "现有记忆不会被直接覆盖。请先检查数据并选择安全恢复方式。",
    "memory.dataSpace.markerVersionUnsupported.title": "记忆数据需要迁移",
    "memory.dataSpace.markerVersionUnsupported.description": "现有数据来自不受支持的版本，需要通过正式迁移流程升级。",
    "memory.dataSpace.schemaMigrationRequired.title": "记忆存储需要升级",
    "memory.dataSpace.schemaMigrationRequired.description": "存储结构与当前版本不一致，需要先完成正式迁移。",
    "memory.dataSpace.rebuildRunning.title": "正在重建记忆索引",
    "memory.dataSpace.rebuildRunning.description": "后台正在安全重建向量索引，完成前请勿关闭应用。",
    "memory.dataSpace.handleLater": "稍后处理",
    "memory.dataSpace.repair": "查看恢复方案",
    "memory.identity.title": "用户与项目",
    "memory.identity.description": "管理长期记忆使用者与项目映射。内部编号仅用于服务路由，不会显示在页面中。",
    "memory.identity.refresh": "刷新列表",
    "memory.identity.users": "用户",
    "memory.identity.projects": "项目",
    "memory.identity.addUser": "添加用户",
    "memory.identity.addProject": "添加项目",
    "memory.identity.userName": "用户名称",
    "memory.identity.teamName": "团队（Team）",
    "memory.identity.spaceName": "空间（Space）",
    "memory.identity.projectName": "项目（Project）",
    "memory.identity.projectPathStructure": "VMM 项目采用 Team / Space / Project 三级路径，每一级都是独立的记忆作用域。",
    "memory.identity.projectPathPreview": "完整路径",
    "memory.identity.projectPlaceholder": "例如：vulcan-code",
    "memory.identity.create": "创建",
    "memory.identity.migrate": "迁移",
    "memory.identity.delete": "删除",
    "memory.identity.default": "默认使用",
    "memory.identity.setDefault": "设为默认",
    "memory.identity.protected": "系统默认",
    "memory.identity.emptyUsers": "尚未创建长期记忆用户。",
    "memory.identity.emptyProjects": "尚未创建长期记忆项目。",
    "memory.identity.confirmDeleteTitle": "确认删除",
    "memory.identity.confirmDeleteDescription": "该操作会删除关联的长期记忆和画像。服务会先检查影响范围，再执行最终删除。",
    "memory.identity.typeName": "输入“{name}”以确认",
    "memory.identity.checking": "正在检查影响范围…",
    "memory.identity.migrateTitle": "迁移项目",
    "memory.identity.migrateDescription": "将源项目中的记忆安全迁移到一个已存在的目标项目。服务会先完成预检。",
    "memory.identity.targetProjectName": "目标项目",
    "memory.common.cancel": "取消",
    "memory.common.close": "关闭",
    "memory.common.save": "保存",
    "memory.common.apply": "应用",
    "memory.common.loading": "处理中…",
    "memory.rules.title": "记忆规则",
    "memory.rules.description": "先调整规则，再一次性应用到所有项目或一个指定项目。",
    "memory.rules.enable": "启用长期记忆",
    "memory.rules.enableDescription": "允许 Vulcan Code 为会话保存和召回长期记忆。",
    "memory.rules.recall": "自动召回",
    "memory.rules.recallDescription": "每轮开始前检索与当前任务相关的记忆。",
    "memory.rules.organize": "自动整理",
    "memory.rules.organizeDescription": "在会话完成后整理并写回有价值的信息。",
    "memory.rules.aiTools": "允许 AI 主动管理记忆",
    "memory.rules.aiToolsDescription": "向当前模型提供记忆搜索、写入与删除工具。",
    "memory.rules.recallTurns": "隐式记忆保留回合",
    "memory.rules.profileRefresh": "画像刷新间隔",
    "memory.rules.profileLimit": "画像条目上限",
    "memory.rules.applySettings": "应用设置",
    "memory.rules.applyTitle": "选择应用范围",
    "memory.rules.applyDescription": "这次修改只会写入一个明确范围，不会批量覆盖未选择的项目。",
    "memory.rules.allProjects": "所有项目（全局默认）",
    "memory.rules.oneProject": "一个指定项目",
    "memory.rules.selectProject": "选择项目",
    "memory.rules.searchProject": "搜索项目…",
    "memory.rules.noProject": "没有可用项目",
    "memory.rules.applied": "设置已应用。",
    "memory.content.title": "记忆内容",
    "memory.content.description": "使用自然语言搜索、查看和维护当前用户与项目的长期记忆。",
    "memory.content.sessionRequiredTitle": "新增记忆需要关联会话",
    "memory.content.sessionRequired": "请先在工作台打开一个具体会话，再返回此处新增记忆。检索和查看现有记忆不受影响。",
    "memory.content.search": "搜索记忆",
    "memory.content.searchPlaceholder": "例如：上次决定使用哪个数据库？",
    "memory.content.add": "新增记忆",
    "memory.content.empty": "输入问题即可搜索长期记忆。",
    "memory.content.noResults": "没有找到相关记忆。",
    "memory.content.details": "查看详情",
    "memory.content.category": "分类",
    "memory.content.createdAt": "创建时间",
    "memory.content.deleteTitle": "删除这条记忆？",
    "memory.content.deleteDescription": "删除后不会再被自动召回。该操作会同时清理对应的向量索引。",
    "memory.content.deleteReason": "删除原因（可选）",
    "memory.content.addTitle": "新增长期记忆",
    "memory.content.summary": "简短摘要",
    "memory.content.fullDetails": "详细内容",
    "memory.content.write": "保存记忆",
    "memory.content.sourceConversation": "来源会话",
    "memory.data.title": "记忆数据",
    "memory.data.description": "按会话和回合查看长期记忆来源，并通过影响预览安全归档、回收或恢复数据。",
    "memory.data.managementReady": "管理服务正常",
    "memory.data.managementUnavailable": "管理服务不可用",
    "memory.data.invalidResponse": "VMM 管理服务返回了不符合契约的数据。",
    "memory.data.activeRuntime": "正在运行，暂不可操作",
    "memory.data.sessions": "会话",
    "memory.data.recycleBin": "回收站",
    "memory.data.memoryEntries": "记忆条目",
    "memory.data.searchPlaceholder": "搜索会话标题或内容摘要",
    "memory.data.statusAll": "全部状态",
    "memory.data.statusActive": "使用中",
    "memory.data.statusArchived": "已归档",
    "memory.data.statusRecycled": "回收站中",
    "memory.data.statusForgotten": "已永久清理",
    "memory.data.allStatuses": "全部提炼状态",
    "memory.data.extractionPending": "等待提炼",
    "memory.data.extractionCompleted": "已提炼",
    "memory.data.extractionFailed": "提炼失败",
    "memory.data.allUsers": "全部记忆用户",
    "memory.data.allProjects": "全部记忆项目",
    "memory.data.createdFrom": "创建起始日期",
    "memory.data.createdTo": "创建结束日期",
    "memory.data.refresh": "刷新",
    "memory.data.resetFilters": "重置筛选",
    "memory.data.fullTextIndex": "全文索引",
    "memory.data.selected": "已选择 {count} 项",
    "memory.data.mixedLifecycle": "所选会话归档状态不同；归档或取消归档需分批操作。",
    "memory.data.archive": "归档",
    "memory.data.unarchive": "取消归档",
    "memory.data.recycle": "移入回收站",
    "memory.data.restartMemory": "重新开始记忆",
    "memory.data.noSessions": "没有符合条件的记忆会话。",
    "memory.data.loadMore": "加载更多",
    "memory.data.viewTurns": "查看回合",
    "memory.data.backToSessions": "返回会话",
    "memory.data.turns": "会话回合",
    "memory.data.noTurns": "该会话没有可见回合。",
    "memory.data.searchTurnPlaceholder": "搜索用户问题或助手回答",
    "memory.data.details": "查看详情",
    "memory.data.turnDetails": "回合详情",
    "memory.data.userContent": "用户内容",
    "memory.data.assistantContent": "助手内容",
    "memory.data.loading": "正在读取…",
    "memory.data.impactTitle": "确认数据影响",
    "memory.data.impactDescription": "执行前请核对将被影响的会话、回合和派生数据。",
    "memory.data.confirmAction": "确认执行",
    "memory.data.cancel": "取消",
    "memory.data.turnCount": "回合",
    "memory.data.memoryCount": "记忆",
    "memory.data.profileCount": "画像",
    "memory.data.vectorCount": "向量",
    "memory.data.pendingCount": "待处理",
    "memory.data.hotCount": "热窗口",
    "memory.data.protectedCount": "受保护",
    "memory.data.recycleEmpty": "回收站中没有可管理的数据。",
    "memory.data.restore": "恢复",
    "memory.data.purge": "永久删除",
    "memory.data.purgeTitle": "永久删除回收数据",
    "memory.data.purgeDescription": "此操作不可恢复。请输入“永久删除”后继续。",
    "memory.data.purgePlaceholder": "永久删除",
    "memory.data.restorable": "可恢复",
    "memory.data.systemBatch": "系统归档，不支持恢复",
    "memory.data.updatedAt": "更新于 {time}",
    "memory.profile.title": "画像",
    "memory.profile.description": "选择一个记忆用户或记忆项目，查看 VMM 整理的长期偏好与约束，并通过自然语言进行修正。",
    "memory.profile.target": "画像目标",
    "memory.profile.user": "用户画像",
    "memory.profile.project": "项目画像",
    "memory.profile.selectUser": "选择记忆用户",
    "memory.profile.selectProject": "选择记忆项目",
    "memory.profile.searchUser": "搜索记忆用户…",
    "memory.profile.searchProject": "搜索记忆项目…",
    "memory.profile.noUser": "没有可用的记忆用户",
    "memory.profile.noProject": "没有可用的记忆项目",
    "memory.profile.refresh": "刷新画像",
    "memory.profile.empty": "当前目标尚未形成画像条目。",
    "memory.profile.instruction": "画像修正指令",
    "memory.profile.instructionPlaceholder": "例如：不要再把旧数据库方案视为当前决定。",
    "memory.profile.applyInstruction": "应用指令",
    "memory.profile.preview": "最终画像预览",
    "memory.profile.previewDescription": "这是提供给记忆服务的完整画像文本，仅在排查或核对时需要查看。",
    "memory.profile.priority": "优先级",
    "memory.profile.level": "层级",
    "memory.advanced.title": "高级设置",
    "memory.advanced.description": "选择本地托管或外部服务，并在需要时查看连接、数据恢复与技术诊断。",
    "memory.advanced.deployment": "运行方式",
    "memory.advanced.managed": "本地托管（推荐）",
    "memory.advanced.managedDescription": "由 Vulcan Code 自动启动、停止和保护本地记忆服务。",
    "memory.advanced.external": "连接外部 VMM",
    "memory.advanced.externalDescription": "连接由你或团队独立运行的 VMM 服务。",
    "memory.advanced.connection": "外部服务连接",
    "memory.advanced.endpoint": "gRPC 地址",
    "memory.advanced.apiKey": "访问密钥",
    "memory.advanced.managementEndpoint": "管理 API 地址",
    "memory.advanced.managementToken": "管理 Token",
    "memory.advanced.testManagement": "保存并测试管理连接",
    "memory.advanced.managementTimeout": "管理请求超时（毫秒）",
    "memory.advanced.runtime": "本地运行",
    "memory.advanced.autoStart": "随 Vulcan Code 自动启动",
    "memory.advanced.manualStart": "仅手动启动",
    "memory.advanced.transport": "连接高级参数",
    "memory.advanced.diagnostics": "技术诊断",
    "memory.advanced.diagnosticRuntime": "运行状态",
    "memory.advanced.diagnosticGrpc": "服务地址",
    "memory.advanced.diagnosticController": "存储控制器",
    "memory.advanced.diagnosticData": "数据目录",
    "memory.advanced.diagnosticLog": "运行日志",
    "memory.advanced.data": "记忆数据",
    "memory.advanced.save": "保存高级设置",
    "memory.advanced.backupRebuild": "备份并重建",
    "memory.advanced.maintenanceUnavailable": "当前版本尚未提供安全维护入口，现有数据不会被修改。",
    "memory.maintenance.confirmTitle": "安全处理历史记忆",
    "memory.maintenance.confirmDescription": "Vulcan Code 将先完整备份现有数据，再使用当前向量模型重建索引。原始数据不会被直接删除。",
    "memory.maintenance.backupFirst": "先备份，后重建",
    "memory.maintenance.backupFirstDescription": "备份完成前不会修改现有向量；重建失败时会保留原始数据和已完成备份。",
    "memory.maintenance.later": "稍后处理",
    "memory.maintenance.progressTitle": "正在处理记忆数据",
    "memory.maintenance.progressDescription": "离开当前页面不会中断操作；重新打开高级设置仍可查看进度。",
    "memory.maintenance.progressCount": "已处理 {processed} / {total}",
    "memory.maintenance.cancel": "取消维护",
    "memory.maintenance.completed": "数据处理完成",
    "memory.maintenance.failed": "数据处理失败",
    "memory.maintenance.cancelled": "数据处理已取消",
    "memory.maintenance.startMemory": "启动记忆服务",
    "memory.maintenance.startFailed": "记忆服务尚未启动",
    "memory.maintenance.startFailedDescription": "备份与重建已经完成。请保留此窗口，并根据页面提示检查运行文件、模型配置或端口后重试。",
    "vmm.experience.deployment.disabled": "未启用",
    "vmm.experience.deployment.managed": "本机托管",
    "vmm.experience.deployment.external": "外部服务",
    "vmm.experience.setup.notConfigured": "未配置",
    "vmm.experience.setup.checking": "正在检查",
    "vmm.experience.setup.ready": "已就绪",
    "vmm.experience.setup.incomplete": "需要完善",
    "vmm.experience.setup.blocked": "需要处理",
    "vmm.experience.lifecycle.stopped": "已停止",
    "vmm.experience.lifecycle.starting": "正在启动",
    "vmm.experience.lifecycle.running": "运行中",
    "vmm.experience.lifecycle.stopping": "正在停止",
    "vmm.experience.lifecycle.restarting": "正在重启",
    "vmm.experience.lifecycle.crashed": "异常退出",
    "vmm.experience.lifecycle.waitingRetry": "等待重试",
    "vmm.experience.lifecycle.failed": "启动失败",
    "vmm.experience.health.unknown": "尚未检查",
    "vmm.experience.health.healthy": "正常",
    "vmm.experience.health.degraded": "部分可用",
    "vmm.experience.health.unavailable": "不可用",
    "vmm.experience.state.available": "可用",
    "vmm.experience.state.unavailable": "不可用",
    "vmm.experience.prerequisite.memory.title": "启用长期记忆",
    "vmm.experience.prerequisite.memory.description": "完成快速配置后，Vulcan Code 才会检索和写入长期记忆。",
    "vmm.experience.prerequisite.transport.title": "配置记忆服务",
    "vmm.experience.prerequisite.transport.description": "选择本机托管或外部服务，并完成连接配置。",
    "vmm.experience.prerequisite.user.title": "建立记忆用户",
    "vmm.experience.prerequisite.user.description": "选择或创建用于长期记忆的用户身份。",
    "vmm.experience.prerequisite.project.title": "关联当前项目",
    "vmm.experience.prerequisite.project.description": "将当前工作区关联到一个长期记忆项目。",
    "vmm.experience.prerequisite.runtime.title": "修复记忆服务启动问题",
    "vmm.experience.prerequisite.runtime.description": "查看受控错误并重试；不会静默覆盖已有记忆数据。",
    "vmm.experience.capability.recall": "对话前自动召回",
    "vmm.experience.capability.writeback": "完成后自动整理",
    "vmm.experience.capability.tools": "AI 记忆工具",
    "vmm.experience.capability.profile": "画像指令",
    "vmm.experience.capability.rerank": "记忆重排",
    "vmm.setup.title": "快速配置长期记忆",
    "vmm.setup.description": "使用本机托管模式建立记忆用户与当前项目。整个过程会先检查数据安全状态。",
    "vmm.setup.stepLabel": "第 {current} 步，共 {total} 步",
    "vmm.setup.previous": "上一步",
    "vmm.setup.next": "下一步",
    "vmm.setup.mode.title": "记忆服务在哪里运行",
    "vmm.setup.mode.description": "大多数用户选择本机托管；已有独立 VMM 服务时再选择外部连接。",
    "vmm.setup.managed.title": "本机托管",
    "vmm.setup.managed.description": "Vulcan Code 负责启动、停止和连接本机 VMM 服务。",
    "vmm.setup.external.title": "连接外部服务",
    "vmm.setup.external.description": "连接已由你或团队运行的 VMM，不启动本机进程。",
    "vmm.setup.inference.title": "选择记忆处理模型",
    "vmm.setup.inference.description": "为记忆提取、整理、向量和重排选择模型与账号。下拉列表会自动过滤到适合对应任务的模型。",
    "vmm.setup.inference.required": "必须先配置并启用向量模型，以及 PreCheck 与 PostAction 的 L1/L2 基础模型和账号。全部路由有效后才会初始化记忆；画像与重排可稍后补充。",
    "vmm.setup.inference.openCredentials": "配置模型账号",
    "vmm.setup.scope.title": "选择启用范围",
    "vmm.setup.scope.description": "先确定记忆身份，再决定立即应用到哪里；以后可在“记忆规则”中调整。",
    "vmm.setup.scope.all_projects": "立即对所有项目启用",
    "vmm.setup.scope.current_project": "仅对当前项目启用",
    "vmm.setup.scope.currentProjectUnavailable": "当前没有可关联的工作区项目",
    "vmm.setup.scope.not_yet": "暂不启用，只完成配置",
    "vmm.setup.confirm.title": "确认配置",
    "vmm.setup.confirm.description": "模型与账号选择已在上一步即时保存；确认后将保存服务位置与启用范围，并创建记忆空间。",
    "vmm.setup.user": "记忆用户名称",
    "vmm.setup.user.placeholder": "例如：本地用户",
    "vmm.setup.project": "当前项目名称",
    "vmm.setup.project.placeholder": "例如：vulcan-code",
    "vmm.setup.safety": "已有数据不会被自动覆盖。发现旧数据、向量空间变化或损坏标记时，配置会暂停并提供明确处理选项。",
    "vmm.setup.cancel": "取消",
    "vmm.setup.submit": "配置并启动",
    "vmm.setup.running": "正在检查、启动并建立记忆空间…",
    "vmm.setup.failed": "快速配置尚未完成，请根据下方提示处理后重试。",
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
    "vmm.runtime.start": "启动",
    "vmm.runtime.restart": "重启",
    "vmm.runtime.stop": "停止",
    "vmm.runtime.openLog": "查看日志",
    "vmm.runtime.mode": "运行方式",
    "vmm.runtime.mode.managed": "Vulcan Code 托管",
    "vmm.runtime.mode.external": "外部 VMM",
    "vmm.runtime.phase.ready": "托管 VMM 已启动",
    "vmm.runtime.phase.starting": "托管 VMM 启动中",
    "vmm.runtime.phase.stopped": "托管 VMM 已停止",
    "vmm.runtime.phase.failed": "托管 VMM 启动失败",
    "vmm.runtime.phase.external": "托管 VMM 未启动",
    "vmm.runtime.phase.crashed": "托管 VMM 已崩溃",
    "vmm.runtime.phase.waiting_retry": "托管 VMM 等待重试",
    "vmm.runtime.phase.stopping": "托管 VMM 正在停止",
    "vmm.runtime.grpcTarget": "实际 gRPC 地址",
    "vmm.runtime.controller": "共享 Controller",
    "vmm.runtime.controllerSpace": "VMM Space",
    "vmm.runtime.dataRoot": "数据目录",
    "vmm.runtime.logPath": "运行日志",
    "vmm.runtime.processCapability": "进程能力",
    "vmm.runtime.scopeCapability": "用户/项目对账",
    "vmm.runtime.profileInstructionCapability": "画像指令",
    "vmm.runtime.capabilityReady": "可用",
    "vmm.runtime.capabilityUnavailable": "不可用",
    "vmm.result.title": "动作结果",
    "vmm.result.emptySummary": "尚未执行 VMM 动作。",
    "vmm.result.request": "请求",
    "vmm.result.response": "响应",
    "vmm.result.emptyHint": "点击左侧动作后会显示请求与响应。",
    "vmm.result.generic": "结果",
    "vmm.managed.behavior.identity": "隐私、噪声与提示词",
    "vmm.managed.behavior.lifecycle": "前置召回与后置写回",
    "vmm.managed.behavior.pipeline": "检索与去重流水线",
    "vmm.managed.behavior.retention": "保留与回收",
    "vmm.managed.piiLanguage": "PII 规则语言",
    "vmm.managed.noiseEnabled": "启用噪声过滤",
    "vmm.managed.noiseLanguage": "噪声规则语言",
    "vmm.managed.noiseSemanticEnabled": "启用语义噪声过滤",
    "vmm.managed.noiseSemanticThreshold": "语义噪声阈值",
    "vmm.managed.promptLanguage": "提示词包",
    "vmm.managed.precheckIntentTimeout": "意图解析超时",
    "vmm.managed.precheckTopK": "前置召回数量",
    "vmm.managed.precheckSearchScope": "前置检索范围",
    "vmm.managed.precheckSimilarityThreshold": "前置相似度阈值",
    "vmm.managed.scope.team": "团队",
    "vmm.managed.scope.space": "空间",
    "vmm.managed.scope.project": "项目",
    "vmm.managed.postactionInputMode": "后置输入模式",
    "vmm.managed.inputMode.compat": "兼容",
    "vmm.managed.inputMode.strict": "严格",
    "vmm.managed.postactionTurnThreshold": "会话分析轮次阈值",
    "vmm.managed.postactionTokenThreshold": "会话分析 Token 阈值",
    "vmm.managed.postactionIdleTimeout": "会话分析空闲超时",
    "vmm.managed.postactionHistoryTurns": "会话分析历史轮数",
    "vmm.managed.postactionMaxInputTokens": "会话分析最大输入 Token",
    "vmm.managed.postactionMaxQueueWorkers": "后置队列工作数",
    "vmm.managed.pipelineMaxSearchKeywords": "最大检索关键词数",
    "vmm.managed.pipelineMinSimilarityScore": "最小召回相似度",
    "vmm.managed.pipelineReplaceMinSimilarityScore": "最小替换相似度",
    "vmm.managed.pipelineHardDedupeThreshold": "硬去重阈值",
    "vmm.managed.pipelineHardDedupePoolTopK": "硬去重候选数",
    "vmm.managed.pipelineHybridEnabled": "启用混合检索",
    "vmm.managed.pipelineLexicalTopK": "词法召回数量",
    "vmm.managed.pipelineRrfK": "倒数排名融合常量",
    "vmm.managed.pipelineMmrEnabled": "启用多样性过滤",
    "vmm.managed.pipelineMmrLambda": "多样性平衡系数",
    "vmm.managed.pipelineWeibullEnabled": "启用 Weibull 衰减",
    "vmm.managed.pipelineWeibullShape": "Weibull 形状参数",
    "vmm.managed.pipelineWeibullScaleHours": "Weibull 小时尺度",
    "vmm.managed.pipelineWeibullMinMultiplier": "最小衰减倍率",
    "vmm.managed.pipelineWeibullReinforceWeight": "强化权重",
    "vmm.managed.pipelineWeibullCrossSessionBoost": "跨会话增强系数",
    "vmm.managed.retentionEnabled": "启用保留扫描",
    "vmm.managed.retentionScanInterval": "保留扫描间隔",
    "vmm.managed.retentionTurnKeepExtraTurns": "热窗口额外轮数",
    "vmm.managed.retentionSessionIdleAfter": "会话空闲回收时长",
    "vmm.managed.retentionTrashDuration": "回收站保留时长",
    "vmm.managed.retentionPriorityFloor": "受保护优先级下限",
    "vmm.managed.retentionMemoryLevelFloor": "受保护记忆级别下限",
    "vmm.managed.retentionSkipProtectedShared": "跳过受保护共享记忆",
    "vmm.settings.grpcTarget": "gRPC 地址",
    "vmm.settings.apiKey": "API Key",
    "vmm.settings.managedRuntime": "由 Vulcan Code 托管本地 VMM",
    "vmm.settings.managedBinaryPath": "VMM 可执行文件",
    "vmm.settings.packagedBinary": "留空使用随包 Sidecar",
    "vmm.settings.autoStartPolicy": "启动策略",
    "vmm.settings.autoStart": "随 Vulcan Code 启动",
    "vmm.settings.manualStart": "手动启动",
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
    "settings.modelProviders": "供应商管理",
    "settings.credentialManagement": "凭据管理",
    "settings.providerCategory.official": "官方",
    "settings.providerCategory.cloudMaaS": "云服务",
    "settings.providerCategory.relay": "中转站",
    "settings.providerCategory.unclassified": "未分类",
    "settings.providerDescription.default": "该供应商提供已注册的模型或 AI 能力接入。",
    "settings.providerDescription.minimax": "MiniMax 官方模型、开放平台与 Token Plan 服务。",
    "settings.providerDescription.kimi": "月之暗面开放平台与 Kimi Coding Plan 服务。",
    "settings.providerDescription.deepseek": "DeepSeek 官方模型 API 与账号服务。",
    "settings.providerDescription.alibaba": "Alibaba Cloud Model Studio、Coding Plan 与 Token Plan 服务。",
    "settings.providerDescription.openrouter": "聚合多个模型与上游节点的统一模型路由平台。",
    "settings.providerDescription.apikeyfun": "提供多种兼容协议的第三方模型中转服务。",
    "settings.providerDescription.claudeapi": "提供 Anthropic 兼容协议的第三方模型中转服务。",
    "settings.providerDescription.qiniu": "七牛云提供的模型推理与兼容协议服务。",
    "settings.providerDescription.fennoai": "提供多种兼容协议的第三方模型中转服务。",
    "settings.providerDescription.code0": "提供 OpenAI 与 Anthropic 兼容协议的第三方模型中转服务。",
    "settings.providerDescription.tavily": "面向 AI 应用的网页搜索与内容抓取服务。",
    "settings.providerDescription.openai": "OpenAI 官方模型与 Codex 账号服务。",
    "settings.providerDescription.anthropic": "Anthropic 官方 Claude 模型与 Claude Code 账号服务。",
    "settings.providerDescription.google": "Google 官方 Gemini API 与 Code Assist 服务。",
    "settings.providerSummary.serviceEntries": "{count} 个接入点",
    "settings.providerSummary.protocols": "{count} 个协议支持",
    "settings.providerSummary.serviceEntriesTitle": "具体接入点",
    "settings.providerSummary.protocolsTitle": "具体协议支持",
    "settings.serviceEntry.title.codingPlan": "Coding Plan",
    "settings.serviceEntry.title.tokenPlan": "Token Plan",
    "settings.serviceEntry.title.modelStudio": "Model Studio",
    "settings.serviceEntry.tag.personal": "个人",
    "settings.serviceEntry.tag.team": "团队",
    "settings.serviceEntry.tag.china": "中国",
    "settings.serviceEntry.tag.international": "国际",
    "settings.providerOnboarding.notConfigured": "待添加凭据",
    "settings.providerOnboarding.description": "供应商入口固定展示；添加真实凭据后才会参与模型同步与调用。",
    "settings.providerOnboarding.serviceEntries": "个可用服务入口",
    "settings.providerOnboarding.register": "注册",
    "settings.providerOnboarding.referralSignup": "推广注册",
    "settings.providerOnboarding.addCredential": "添加凭据",
    "settings.providerConnection.serviceOrigin": "工作空间专属入口（可选）",
    "settings.providerConnection.serviceOriginDescription": "可粘贴控制台显示的完整 OpenAI 或 Anthropic 地址；留空时使用公共入口。该设置只作用于当前凭据。",
    "settings.providerOnboarding.filteredEmpty": "当前供应商没有凭据",
    "settings.providerOnboarding.filteredEmptyDescription": "可切换到其他供应商，或通过顶部按钮添加新凭据。",
    "settings.providerOnboarding.externalConfirmation": "即将打开供应商外部注册页面，是否继续？",
    "settings.providerOnboarding.externalOpenAnyway": "仍要打开",
    "settings.providerOnboarding.externalOpenFailed": "打开供应商注册页面失败",
    "settings.providerPromotion.neutralDisclosure": "普通注册链接，不包含 Vulcan 收益归因。",
    "settings.providerPromotion.referralDisclosure": "通过邀请链接注册可能会为开发者带来收益，用于支持 Vulcan 持续开发；不会影响模型排序或调用路由。",
    "settings.providerPromotion.sponsoredDisclosure": "赞助合作入口；合作关系不会影响模型排序或调用路由。",
    "settings.modelList": "模型列表",
    "settings.providerRetry": "请求重试",
    "settings.aiVendors": "能力供应商",
    "settings.aiWebSearch": "网页搜索",
    "settings.aiWebExtract": "网页抓取",
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
    "settings.modelProviders.description": "查看全部已实现供应商，并管理自定义兼容供应商。",
    "settings.credentialManagement.description": "管理模型账号、调用协议、用量与默认凭据。",
    "settings.modelList.description": "按供应商查看已启用模型，并核对当前工作台模型。",
    "settings.providerRetry.description": "配置普通错误与 429 限流的重试次数、等待间隔和递增规则。",
    "settings.aiVendors.description": "多个语义 AI 工具共用的能力供应商与路由目标。",
    "settings.aiWebSearch.description": "网页搜索能力的启用状态与可用目标。",
    "settings.aiWebExtract.description": "网页抓取能力的启用状态与可用目标。",
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
    "settings.modelOverrides.title": "模型附加条件",
    "settings.modelOverrides.description": "调整当前供应商、模型与协议的运行参数，不改变账号选择。",
    "settings.modelOverrides.limits": "Token 限制",
    "settings.modelOverrides.limitsDescription": "供应商已声明上限时只能向下收窄；未声明时允许手动设定，留空则使用供应商默认行为。",
    "settings.modelOverrides.contextLimit": "上下文限制",
    "settings.modelOverrides.outputLimit": "输出 Token 限制",
    "settings.modelOverrides.leaveBlank": "留空使用供应商默认值",
    "settings.modelOverrides.providerMaximum": "供应商上限",
    "settings.modelOverrides.providerNotDeclared": "供应商未声明上限，可手动设置覆盖值。",
    "settings.modelOverrides.cache": "提示词缓存",
    "settings.modelOverrides.cacheDescription": "选择供应商默认行为，或在协议支持时由 Vulcan 显式管理。",
    "settings.modelOverrides.cacheMode": "缓存策略",
    "settings.modelOverrides.providerDefault": "供应商默认，不额外注入",
    "settings.modelOverrides.managedExplicit": "Vulcan 显式管理",
    "settings.modelOverrides.cacheTtl": "缓存 TTL",
    "settings.modelOverrides.parameters": "模型参数",
    "settings.modelOverrides.parametersDescription": "这里只显示当前协议明确注册的参数。",
    "settings.modelOverrides.noParameters": "当前协议没有额外模型参数。",
    "settings.modelOverrides.unsupportedParameter": "该复杂参数尚未注册可视化编辑器",
    "settings.modelOverrides.enabled": "启用",
    "settings.customModels.title": "{provider} · 模型管理",
    "settings.customModels.description": "模型定义独立于凭据保存；目录导入和手动修改只写用户覆盖，不会改写同步来源。",
    "settings.customModels.pageModels": "模型",
    "settings.customModels.pageProviderSettings": "供应商配置",
    "settings.customModels.search": "搜索模型名称或 ID",
    "settings.customModels.filterAll": "全部模型",
    "settings.customModels.filterEnabled": "仅已启用",
    "settings.customModels.filterDisabled": "仅已关闭",
    "settings.customModels.importDiscovered": "从当前目录导入缺失模型",
    "settings.customModels.importTitle": "选择要导入的模型",
    "settings.customModels.importDescription": "仅选中的目录模型会复制到用户覆盖层；同步来源不会被修改。",
    "settings.customModels.importSelected": "导入 {count} 个模型",
    "settings.customModels.addManual": "手动添加模型",
    "settings.customModels.enableAll": "全部启用",
    "settings.customModels.disableAll": "全部禁用",
    "settings.customModels.enabled": "启用",
    "settings.customModels.disabled": "关闭",
    "settings.credentialModels.manage": "管理此账号的模型",
    "settings.credentialModels.title": "账号模型",
    "settings.credentialModels.description": "只控制此账号可调用的模型，不会修改供应商目录，也不会影响同一供应商的其他账号。",
    "settings.credentialModels.search": "搜索模型名称或 ID",
    "settings.credentialModels.filterAll": "全部",
    "settings.credentialModels.filterEnabled": "已启用",
    "settings.credentialModels.filterDisabled": "已关闭",
    "settings.credentialModels.enableAll": "全部启用",
    "settings.credentialModels.disableAll": "全部关闭",
    "settings.credentialModels.sync": "同步目录",
    "settings.credentialModels.syncing": "正在同步",
    "settings.credentialModels.count": "已启用 {enabled} / {total}",
    "settings.credentialModels.empty": "此账号目前没有可管理的模型。",
    "settings.credentialModels.noResults": "没有符合当前搜索和筛选条件的模型。",
    "settings.credentialModels.directoryFixed": "内置目录",
    "settings.credentialModels.directoryDynamic": "账号目录",
    "settings.credentialModels.directoryManual": "手工目录",
    "settings.credentialModels.directoryUnavailable": "无模型目录",
    "settings.credentialModels.disableConfirmTitle": "关闭此账号的全部模型？",
    "settings.credentialModels.disableConfirmDescription": "此账号将不再参与模型路由；新会话和已加载会话会改用仍可用的账号。供应商目录与其他账号不会改变。",
    "settings.credentialModels.cancel": "取消",
    "settings.credentialModels.confirmDisable": "关闭全部模型",
    "settings.credentialModels.futureEnabled": "后续同步发现的新模型会自动启用。",
    "settings.credentialModels.futureDisabled": "后续同步发现的新模型保持关闭，直到你手动启用。",
    "settings.customModels.empty": "当前筛选条件下没有模型。",
    "settings.customModels.noModelsToImport": "当前目录没有可追加的新模型。",
    "settings.customModels.removeConfirm": "确定从自定义供应商中移除模型“{name}”吗？如果模型被会话锁定或被内部推理路由引用，后端会禁止删除，请先切换相关会话或路由。",
    "settings.customProviders.removeTitle": "移除供应商",
    "settings.customProviders.removeDescription": "确定移除自定义供应商“{name}”吗？确认后将直接删除该供应商及其全部专属配置。",
    "settings.customProviders.removeModelsWarning": "将同时删除 {count} 项模型自定义信息或路由设置。",
    "settings.customProviders.removeModelsWarningGeneric": "将同时删除该供应商的模型自定义信息、账号模型策略和内部路由设置。",
    "settings.customProviders.removeCredentialsWarning": "确认后将同时删除该供应商的 {count} 个凭据。",
    "settings.customProviders.removeSessionsWarning": "如果已有会话使用该供应商，移除后会话模型将失效，并自动切换到默认模型或第一个可用模型。",
    "settings.customProviders.modelCount": "{count} 个模型",
    "settings.customProviders.manageModels": "管理模型",
    "settings.customProviders.manageModelsFor": "管理 {provider} 的模型",
    "settings.customProviders.addTitle": "添加自定义供应商",
    "settings.customProviders.editTitle": "编辑自定义供应商",
    "settings.customProviders.dialogDescription": "一个基础 URL 只绑定一种调用协议。先确认入口，再用真实凭据验证模型目录；新增时该凭据会成为首个账号。",
    "settings.customProviders.progressLabel": "自定义供应商配置进度",
    "settings.customProviders.step.connection": "入口",
    "settings.customProviders.step.credential": "凭据",
    "settings.customProviders.step.result": "确认",
    "settings.customProviders.displayName": "显示名称",
    "settings.customProviders.baseUrl": "基础 URL",
    "settings.customProviders.protocol": "调用协议",
    "settings.customProviders.codexCapabilities": "Codex Responses 能力",
    "settings.customProviders.codexWebsocket": "优先使用 WebSocket",
    "settings.customProviders.codexWebsocketHint": "对端不支持时，在产生语义输出前回退到 HTTP。",
    "settings.customProviders.codexRemoteCompaction": "启用远端上下文压缩",
    "settings.customProviders.codexRemoteCompactionHint": "调用 /responses/compact；仅在未产生语义输出或副作用且可安全重试时回退到本地压缩。",
    "settings.customProviders.authentication": "鉴权请求头",
    "settings.customProviders.discoveryMode": "模型目录",
    "settings.customProviders.discoveryStandard": "标准 /models",
    "settings.customProviders.discoveryCustomPath": "自定义同源路径",
    "settings.customProviders.discoveryManual": "不支持目录，手动维护",
    "settings.customProviders.discoveryPath": "模型目录路径",
    "settings.customProviders.protocolEndpoint": "调用端点：",
    "settings.customProviders.modelsEndpoint": "目录端点：",
    "settings.customProviders.manualOnly": "不访问远程目录",
    "settings.customProviders.validationTitle": "使用真实凭据验证",
    "settings.customProviders.validationEditDescription": "先选择一个已有账号预检。保存协议、地址、鉴权或发现方式变更时，系统会使用各账号自身凭据逐一验证并原子迁移；不会创建账号、要求重新输入密钥或替换凭据。仅修改名称不会触发重验。",
    "settings.customProviders.validationCreateDescription": "验证成功后，该凭据会作为此供应商的首个账号保存。",
    "settings.customProviders.validationCredentialName": "验证凭据名称",
    "settings.customProviders.validationCredentialRequired": "选择一个已有账号",
    "settings.customProviders.credentialFreeEditTitle": "当前没有账号，可直接保存",
    "settings.customProviders.credentialFreeEditDescription": "该供应商暂无凭据，无法执行远程验证。你仍可保存协议、地址、鉴权和模型目录配置；后续添加账号时将按这份配置验证。",
    "settings.customProviders.firstCredentialName": "首个凭据名称",
    "settings.customProviders.apiKey": "API Key",
    "settings.customProviders.apiKeyEditPlaceholder": "仅用于验证本次配置",
    "settings.customProviders.apiKeyCreatePlaceholder": "验证通过后安全保存",
    "settings.customProviders.credentialRequired": "请填写凭据名称和有效 API Key。",
    "settings.customProviders.back": "上一步",
    "settings.customProviders.next": "下一步",
    "settings.customProviders.probing": "正在验证…",
    "settings.customProviders.probe": "验证并同步模型",
    "settings.customProviders.validationResultDescription": "验证完成；请选择与当前状态匹配的保存方式。",
    "settings.customProviders.validationModelCount": "发现模型数",
    "settings.customProviders.validationPageCount": "读取分页数",
    "settings.customProviders.validationCredential": "验证凭据",
    "settings.customProviders.validationObservedAt": "验证时间",
    "settings.customProviders.validationModelsTitle": "已发现模型确认",
    "settings.customProviders.validationEvidence": "能力证据 {known}/{total}",
    "settings.customProviders.validationModelsHint": "只保存供应商、凭据与用户配置；此远程模型目录仅保留在当前进程内。",
    "settings.customProviders.manualFallbackHint": "该结果允许暂存入口并转入手动模型管理；凭据会保持待验证隔离，直至一次明确的模型测试成功。",
    "settings.customProviders.pendingSaveHint": "这是限流或临时故障，可明确暂存配置并在稍后重新同步。",
    "settings.customProviders.saving": "正在保存…",
    "settings.customProviders.saveVerified": "保存并继续",
    "settings.customProviders.saveManual": "保存并手动配置模型",
    "settings.customProviders.savePending": "暂存并稍后重试",
    "settings.customProviders.pendingConfirmTitle": "暂存尚未验证的连接？",
    "settings.customProviders.pendingConfirmDescription": "该凭据尚未完成模型目录验证。保存后它会保持隔离，不能成为默认账号或进入会话路由；必须稍后同步目录并成功执行一次明确的模型测试。",
    "settings.customProviders.confirmPending": "确认暂存",
    "settings.customProviders.protocolOverridesConfirmTitle": "删除旧协议的模型设置？",
    "settings.customProviders.protocolOverridesConfirmDescription": "切换到 {protocol} 后，另一个协议下的 {count} 项模型设置无法继续应用。确认后会删除这些不兼容设置；目标协议已有设置不会受影响。",
    "settings.customProviders.protocolOverridesConfirmAction": "删除并切换协议",
    "settings.providerModels.description": "这里决定供应商向账号和模型列表发布哪些模型。开关修改会立即保存；已关闭模型只保留在此管理窗口中。",
    "settings.providerModels.count": "已发布 {enabled} / {total}",
    "settings.providerModels.empty": "当前凭据尚未同步出可管理的模型。",
    "settings.customProviders.displayNameRequired": "请输入供应商显示名称。",
    "settings.customProviders.baseUrlInvalid": "请输入有效的 HTTP 或 HTTPS 基础 URL。",
    "settings.customProviders.discoveryPathInvalid": "自定义目录路径必须以 / 开头，且不能包含域名或上级目录。",
    "settings.customProviders.discovery.standard": "标准目录",
    "settings.customProviders.discovery.custom_path": "自定义路径",
    "settings.customProviders.discovery.manual": "手动维护",
    "settings.customModels.identityRequired": "每个模型都必须填写模型 ID 和显示名称。",
    "settings.customModels.duplicateId": "模型 ID 必须唯一，请修改重复的 ID：{model}。",
    "settings.customModels.parametersInvalid": "模型参数必须是 JSON 数组。",
    "settings.customModels.basic": "基础信息",
    "settings.customModels.basicHint": "上下文默认采用公共能力库；手动模型从 200,000 开始，可直接修改。",
    "settings.customModels.modelId": "模型 ID",
    "settings.customModels.displayName": "显示名称",
    "settings.customModels.contextLimit": "上下文限制",
    "settings.customModels.outputLimit": "输出 Token 限制（可选）",
    "settings.customModels.modalities": "输入与输出",
    "settings.customModels.modalitiesHint": "文本是所有会话模型的基础能力；这里只声明额外资源模态。",
    "settings.customModels.input": "输入资源",
    "settings.customModels.output": "输出资源",
    "settings.customModels.behavior": "工具与思考",
    "settings.customModels.behaviorHint": "严格声明供应商实际支持形态；未知不会被当作支持。",
    "settings.customModels.toolCalling": "工具调用",
    "settings.customModels.parallelTools": "并行工具调用",
    "settings.customModels.reasoning": "思考支持",
    "settings.customModels.reasoningControl": "思考控制方式",
    "settings.customModels.supportUnknown": "尚未声明",
    "settings.customModels.supportNative": "原生支持",
    "settings.customModels.supportEquivalent": "等效支持",
    "settings.customModels.supportLossy": "有限兼容",
    "settings.customModels.supportUnsupported": "不支持",
    "settings.customModels.controlUnknown": "尚未声明",
    "settings.customModels.controlFixedOff": "固定关闭",
    "settings.customModels.controlFixedOn": "固定开启",
    "settings.customModels.controlToggle": "允许开关",
    "settings.customModels.controlEffortSelection": "思考档位",
    "settings.customModels.controlProviderAdaptive": "供应商自适应",
    "settings.customModels.reasoningEfforts": "可选思考档位",
    "settings.customModels.effortNone": "关闭（none）",
    "settings.customModels.effortAdaptive": "自适应（adaptive）",
    "settings.customModels.effortMinimal": "最小（minimal）",
    "settings.customModels.effortLow": "低（low）",
    "settings.customModels.effortMedium": "中（medium）",
    "settings.customModels.effortHigh": "高（high）",
    "settings.customModels.effortExtraHigh": "超高（xhigh）",
    "settings.customModels.effortMax": "最大（max）",
    "settings.customModels.effortUltra": "极致（ultra）",
    "settings.customModels.advanced": "高级协议映射",
    "settings.customModels.advancedHint": "仅在确认供应商请求字段后填写。宿主持有字段会由后端拒绝，避免覆盖模型、消息和工具。",
    "settings.customModels.parameters": "模型参数定义（JSON 数组）",
    "settings.customModels.projection": "请求投影（JSON 对象）",
    "settings.customModels.projectionInvalid": "请求投影必须是 JSON 对象。",
    "settings.customModels.remove": "移除当前模型",
    "settings.customModels.selectHint": "从左侧选择模型，或从目录导入、手动添加一个模型。",
    "settings.customModels.save": "保存模型",
    "settings.customModels.defaultsTitle": "供应商模型设置",
    "settings.customModels.defaultsDescription": "直接调整此供应商所有对话模型的默认值；模型设置仍可覆盖这些值。",
    "settings.customModels.defaultsEdit": "编辑默认值",
    "settings.customModels.contextSummary": "上下文 {count}",
    "settings.customModels.outputSummary": "输出 {count}",
    "settings.customModels.textInputOutput": "文本输入 / 文本输出",
    "settings.customModels.outputRecommendation": "按当前上下文建议输出上限为 {count} Token（最多 32,768）。",
    "settings.customModels.defaultsContextInvalid": "上下文限制必须是大于 0 的整数。",
    "settings.customModels.defaultsOutputInvalid": "输出限制必须是大于 0 且不超过上下文限制的整数。",
    "settings.customModels.defaultsReasoningInvalid": "思考开启时至少需要保留一个思考档位。",
    "settings.customModels.defaultsCustomSource": "供应商设置",
    "settings.customModels.defaultsSystemSource": "系统默认",
    "settings.customModels.defaultsSave": "保存供应商设置",
    "settings.customModels.fixedTextOutput": "固定文本输出",
    "settings.customModels.fixedTextOutputDescription": "通用对话供应商只支持文本输出；图片等专用输出由原生系统供应商声明。",
    "settings.customModels.toolCallingDefaultDescription": "工具调用默认开启；仅在供应商或模型明确不支持时关闭。",
    "settings.customModels.templateSearching": "正在本地能力库中检查精确模型匹配…",
    "settings.customModels.templateUpdateAvailable": "公共能力模板已有新版本",
    "settings.customModels.templateUpdateDescription": "当前模型继续使用已确认的旧快照；只有点击应用更新后才会切换到新版本。",
    "settings.customModels.templateCurrent": "当前快照",
    "settings.customModels.templateLatest": "最新模板",
    "settings.customModels.templateApplyUpdate": "应用模板更新",
    "settings.customModels.templateApplied": "已应用公共模型模板",
    "settings.customModels.templateRestore": "重新选择",
    "settings.customModels.templateNone": "没有可靠的公共模板",
    "settings.customModels.templateDefaultFallback": "将使用当前供应商的默认上下文、输出上限和文本能力。",
    "settings.customModels.templateRejected": "已明确不使用公共模板",
    "settings.customModels.templateReviewAgain": "重新检查",
    "settings.customModels.templateSuggested": "发现可复用的公共能力模板",
    "settings.customModels.templateApply": "应用此模板",
    "settings.customModels.templateKeepDefaults": "保持供应商默认值",
    "settings.customModels.templateMatch.exact": "模型 ID 完全匹配",
    "settings.customModels.templateMatch.alias": "已知别名匹配",
    "settings.customModels.templateMatch.provider_agnostic": "忽略供应商前缀后匹配",
    "settings.customModels.discoveryTitle": "账号目录同步",
    "settings.customModels.discoverySummary": "{accounts} 个账号共发现 {models} 个模型，{failed} 个账号需要处理；目录只缓存在内存中。",
    "settings.customModels.reviewTemplates": "审查能力建议",
    "settings.customModels.reviewTemplatesTitle": "审查模型能力建议",
    "settings.customModels.reviewTemplatesDescription": "只显示精确键、明确别名与去供应商前缀后的唯一精确命中；未命中的模型保留供应商默认值。",
    "settings.customModels.reviewExact": "可批量应用",
    "settings.customModels.reviewMissing": "使用供应商默认值",
    "settings.customModels.reviewExactDescription": "精确或确定性命中",
    "settings.customModels.reviewMissingDescription": "无命中：继续使用 200K/32K 纯文本供应商默认值",
    "settings.customModels.reviewComplete": "当前模型均已审查，或没有待处理的模板建议。",
    "settings.customModels.applyAllExact": "应用全部确定性建议（{count}）",
    "settings.customModels.refreshAll": "同步全部账号",
    "settings.customModels.refreshAccount": "同步此账号",
    "settings.customModels.lastSynchronized": "最近同步：{time}",
    "settings.customModels.notSynchronized": "尚未同步",
    "settings.customModels.discoveryDetails": "账号同步详情与诊断",
    "settings.customModels.discoveryModelCount": "{count} 个模型",
    "settings.customModels.discoveryPageCount": "{count} 页",
    "settings.customModels.discoverySchema": "解析契约：{revision}",
    "settings.customModels.discovery.notChecked": "尚未检查",
    "settings.customModels.discovery.ready": "目录就绪",
    "settings.customModels.discovery.empty": "目录为空",
    "settings.customModels.discovery.unsupported": "不支持目录接口",
    "settings.customModels.discovery.authenticationFailed": "凭据验证失败",
    "settings.customModels.discovery.rateLimited": "请求受限",
    "settings.customModels.discovery.temporarilyUnavailable": "暂时不可用",
    "settings.customModels.discovery.invalidResponse": "响应格式无效",
    "settings.customModels.discovery.stale": "使用上次内存结果",
    "settings.customModels.discovery.failed": "同步失败",
    "settings.customModels.sourceApi": "API 目录",
    "settings.customModels.sourceManual": "手动声明",
    "settings.customModels.sourceUserOverride": "模型覆盖",
    "settings.customModels.sourceAccount": "账号目录证据",
    "settings.customModels.sourceTemplate": "公共模板",
    "settings.customModels.sourceProviderDefault": "供应商默认值",
    "settings.customModels.backToList": "返回模型列表",
    "settings.customModels.tabOverview": "概览",
    "settings.customModels.tabAccounts": "账号",
    "settings.customModels.tabCapabilities": "能力",
    "settings.customModels.tabAdvanced": "高级",
    "settings.customModels.accountReturned": "目录已返回此模型",
    "settings.customModels.accountNotReturned": "目录未返回此模型",
    "settings.customModels.accountAvailable": "当前可用",
    "settings.customModels.accountUnavailable": "当前不可用",
    "settings.customModels.accountInherit": "跟随目录",
    "settings.customModels.accountForceEnable": "强制允许",
    "settings.customModels.accountForceDisable": "强制禁用",
    "settings.customModels.capabilityAccount": "查看账号能力",
    "settings.customModels.viewAccountCapabilities": "查看能力",
    "settings.customModels.pendingValidation": "待模型验证",
    "settings.customModels.pendingCannotDefault": "待验证账号不能设为默认，请先完成模型测试",
    "settings.customModels.validateModel": "测试模型",
    "settings.customModels.validateModelTitle": "发送一次真实模型测试？",
    "settings.customModels.validateModelDescription": "将通过账号“{account}”向模型“{model}”发送一条最小文本请求。此操作可能产生少量费用；只有成功收到可见文本后，待验证账号才会进入会话路由。",
    "settings.customModels.confirmValidation": "确认并测试",
    "settings.customModels.validationComplete": "模型测试成功",
    "settings.customModels.validationSucceeded": "账号“{account}”已通过测试，模型返回：{response}",
    "settings.customModels.noAccounts": "尚无可用账号",
    "settings.customModels.noAccountsDescription": "添加并验证凭据后，才能查看逐账号能力与可用性。",
    "settings.customModels.saveCapabilityOverrides": "保存能力覆盖",
    "settings.customModels.changeSummaryTitle": "确认变更摘要",
    "settings.customModels.changeSummaryDescription": "请确认模型“{model}”即将保存的设置。",
    "settings.customModels.confirmChanges": "确认保存",
    "settings.customModels.summaryModel": "模型",
    "settings.customModels.summaryAccount": "账号",
    "settings.customModels.summaryAvailability": "账号可用性",
    "settings.customModels.summaryRoutingState": "路由状态",
    "settings.customModels.summaryCapabilityOverrides": "能力覆盖",
    "settings.customModels.summaryRemoveOverrides": "移除全部用户能力覆盖，恢复来源优先级解析",
    "settings.customModels.summaryTemplate": "能力来源",
    "settings.customModels.summaryDefaults": "默认上下文 / 输出",
    "settings.customModels.summaryNotEntered": "尚未输入",
    "settings.customModels.advancedChangeSummaryDescription": "请确认将一次保存的全部高级模型定义。未列出的目录事实不会被复制或覆盖。",
    "settings.customModels.summaryAdvancedDefinitions": "高级定义",
    "settings.customModels.summaryCurrentModel": "当前模型",
    "settings.customModels.textOnly": "仅文本",
    "settings.customModels.advancedUnavailable": "目录模型尚未创建高级定义。",
    "settings.customModels.advancedCreateHint": "只有在需要协议参数或请求投影时才创建用户定义；模型目录本身不会被复制。",
    "settings.customModels.advancedCreate": "创建高级定义",
    "settings.customModels.saveAdvanced": "保存高级定义",
    "settings.customModels.unsavedAdvanced": "高级定义有尚未保存的修改。",
    "settings.customModels.modelIdRequired": "请输入真实模型 ID。",
    "settings.customModels.manualDescription": "仅在 /models 不可用或没有返回目标模型时手动添加。系统会先在本地公共库中寻找模板。",
    "settings.customModels.templateChoice": "能力模板",
    "settings.customModels.discardTitle": "放弃未保存的修改？",
    "settings.customModels.discardDescription": "高级模型定义仍有未保存修改。关闭后这些修改将丢失。",
    "settings.customModels.discard": "放弃修改",
"settings.modelCapabilities.title": "模型设置",
    "settings.modelCapabilities.shortLabel": "能力",
"settings.modelCapabilities.overlayHint": "当前显示有效值；保存后由此模型直接覆盖供应商设置。",
  "settings.modelCapabilities.contextLimit": "上下文限制",
  "settings.modelCapabilities.overrideContext": "覆盖公共库或供应商声明的上下文限制",
    "settings.modelCapabilities.contextHint": "没有独立声明时公共库回退为 200,000；此处可以按实际套餐向下或向上覆盖。",
    "settings.modelCapabilities.contextInvalid": "上下文限制必须是大于 0 的整数。",
    "settings.modelCapabilities.inputResources": "输入资源覆盖",
    "settings.modelCapabilities.outputResources": "输出资源覆盖",
    "settings.modelCapabilities.overrideResources": "启用完整集合覆盖（可显式选择空集合）",
"settings.modelCapabilities.restore": "恢复来源默认值",
    "settings.modelCapabilities.modelSource": "模型设置",
    "settings.modelCapabilities.inheritedSource": "继承供应商",
    "settings.contextWindowPolicy.title": "Codex 上下文窗口策略",
    "settings.contextWindowPolicy.description": "按原始模型窗口计算：默认 95% 有效窗口、90% 自动压缩线；留空表示继承上级设置。",
    "settings.contextWindowPolicy.effectivePercent": "有效窗口百分比",
    "settings.contextWindowPolicy.autoCompactPercent": "自动压缩百分比",
    "settings.contextWindowPolicy.autoCompactTokenLimit": "自动压缩 Token 上限",
    "settings.contextWindowPolicy.scope": "压缩计量范围",
    "settings.contextWindowPolicy.scopeInherit": "继承上级",
    "settings.contextWindowPolicy.scopeTotal": "总用量（Total）",
    "settings.contextWindowPolicy.scopeBodyAfterPrefix": "稳定前缀之后（BodyAfterPrefix）",
    "settings.contextWindowPolicy.fallbackBuffer": "回退缓冲 Token",
    "settings.contextWindowPolicy.clear": "清除本层策略",
    "settings.models.enabled": "已启用",
    "settings.models.disabled": "已关闭",
    "settings.models.toggle": "切换模型启用状态",
    "settings.models.loadingCatalog": "正在读取实时模型目录…",
    "settings.models.provider": "供应商",
    "settings.models.liveModels": "实时模型",
    "settings.models.noProviderSelected": "未选择供应商",
    "settings.models.catalogSummary": "{provider} · 总计 {count}",
    "settings.models.memoryOnly": "仅保存在内存；启动和手动同步后更新",
    "settings.models.emptyCatalog": "当前没有已同步的模型。请先在“模型供应商”中添加账号并同步。",
    "settings.models.availableAccounts": "{model} · {count} 个可用账号",
    "settings.models.availableCredentials": "{count} 个可用凭据",
    "settings.models.status": "状态",
    "settings.models.additionalConditions": "{protocol} · 附加条件",
    "settings.models.collapseProviders": "收起为胶囊",
    "settings.models.expandProviders": "展开供应商分类",
    "settings.models.searchLabel": "搜索模型",
    "settings.models.searchPlaceholder": "搜索模型名称或 ID",
    "settings.models.statusFilter": "按启用状态筛选模型",
    "settings.models.filterAll": "全部状态",
    "settings.models.filterEnabled": "仅已启用",
    "settings.models.filterDisabled": "仅已关闭",
    "settings.models.enableAll": "全部启用",
    "settings.models.disableAll": "全部禁用",
    "settings.models.filteredSummary": "显示 {visible} / {total} 个模型",
    "settings.models.emptyFilter": "当前筛选条件下没有模型。",
    "settings.models.inputModalities": "输入",
    "settings.models.outputModalities": "输出",
    "settings.models.modality.text": "文本",
    "settings.models.modality.image": "图片",
    "settings.models.modality.audio": "音频",
    "settings.models.modality.video": "视频",
    "settings.models.modality.file": "文件",
    "settings.models.modality.embedding": "向量",
    "settings.models.modality.rerank": "重排",
    "settings.inference.title": "内部推理",
    "settings.inference.subtitle": "为 VMM 等内部消费者选择精确模型、协议和凭据，不跟随当前会话模型。",
    "settings.inference.vmmReasoningNotice": "为保证记忆处理效率，VMM 的全部 LLM 请求都会强制关闭思考；模型列表仅显示明确支持关闭思考的模型。",
    "settings.inference.profileVmm": "VMM 配置档",
    "settings.inference.llm": "受限 LLM",
    "settings.inference.embedding": "向量",
    "settings.inference.rerank": "重排",
    "settings.inference.noRoute": "未配置",
    "settings.inference.modelRegion": "模型",
    "settings.inference.accountRegion": "账号",
    "settings.inference.selectModel": "选择模型",
    "settings.inference.selectAccount": "选择账号",
    "settings.inference.selectModelFirst": "请先选择模型",
    "settings.inference.accountFilter": "输入账号、服务入口或协议过滤",
    "settings.inference.noAccountMatches": "当前模型没有匹配账号。",
    "settings.inference.unavailable": "当前不可用",
    "settings.inference.unavailableReason": "当前路由不可执行：{reason}",
    "settings.inference.testConnection": "执行真实连接测试",
    "settings.inference.testEmbeddingSuccess": "测试通过：{count} 项 · {dimensions} 维 · {duration} 毫秒",
    "settings.inference.testRerankSuccess": "测试通过：{count} 项排序结果 · {duration} 毫秒",
    "settings.inference.credentialAvailable": "凭据可用",
    "settings.inference.catalogGeneration": "目录代次 {generation}",
    "settings.inference.capabilityRevision": "能力修订 {revision}",
    "settings.inference.scopeInteractive": "交互式智能体",
    "settings.inference.scopeBackground": "内部后台",
    "settings.inference.scopeExternal": "外部服务",
    "settings.inference.internalToggle": "切换内部推理能力",
    "settings.inference.configureDefaults": "配置内部默认参数",
    "settings.inference.overridesTitle": "内部推理默认参数",
    "settings.inference.overridesDescription": "这些设置属于供应商模型和操作，不绑定具体账号；保存时会校验全部精确可用路由。",
    "settings.inference.inheritModelDefault": "使用模型默认值",
    "settings.inference.reasoningEffort": "思考档位",
    "settings.inference.embeddingTask": "向量任务",
    "settings.inference.embeddingDimensions": "向量维度",
    "settings.inference.embeddingOutputKind": "向量输出类型",
    "settings.inference.embeddingEncoding": "向量编码",
    "settings.inference.rerankTopN": "默认返回数量",
    "settings.inference.rerankTruncation": "截断策略",
    "settings.inference.rerankReturnContent": "返回候选正文",
    "settings.inference.rerankRestartTitle": "修改重排设置并重启 VMM？",
    "settings.inference.rerankRestartDescription": "启用、关闭或更换重排模型会写入新配置，并需要重启 VMM 才能生效。是否确认继续？",
    "settings.inference.rerankRestartCancel": "取消",
    "settings.inference.rerankRestartConfirm": "保存并重启",
    "settings.inference.rerankRestartFailed": "重排配置已保存，但 VMM 重启失败；请从 VMM 状态页重试。",
    "settings.inference.maintenanceLocked": "记忆备份与向量重建进行中，推理路由暂时锁定，避免并发修改数据空间。",
    "settings.inference.purpose.precheckL1": "快速判断",
    "settings.inference.purpose.precheckL1Description": "PreCheck L1：低延迟判断本轮是否需要召回长期记忆。",
    "settings.inference.purpose.precheckL2": "深度判断",
    "settings.inference.purpose.precheckL2Description": "PreCheck L2：在复杂输入下补充判断应召回哪些长期记忆。",
    "settings.inference.purpose.postactionL1": "快速整理",
    "settings.inference.purpose.postactionL1Description": "PostAction L1：低延迟提取本轮值得保留的信息。",
    "settings.inference.purpose.postactionL2": "深度整理",
    "settings.inference.purpose.postactionL2Description": "PostAction L2：复核、合并并形成可长期使用的记忆。",
    "settings.inference.purpose.profileInstruction": "画像指令",
    "settings.inference.purpose.profileInstructionDescription": "处理用户对个人或项目画像提出的自然语言修正。",
    "settings.inference.purpose.reserve": "备用任务",
    "settings.inference.purpose.reserveDescription": "预留给 VMM 后续内部任务，不影响当前常规记忆流程。",
    "settings.providerTools.title": "AI 扩展能力",
    "settings.providerTools.description": "每项能力只能选择一个实际模型；AI 将收到该模型的原生参数 Schema。",
    "settings.providerTools.select": "选择模型",
    "settings.providerTools.disabled": "不启用",
    "settings.providerTools.unavailable": "已选目标不可用",
    "settings.providerTools.imageGenerate": "图片生成",
    "settings.providerTools.imageEdit": "图片编辑工具",
    "settings.providerTools.mediaAnalyze": "图片理解工具",
    "settings.providerTools.musicGenerate": "音乐生成",
    "settings.providerTools.musicCover": "音乐翻唱",
    "settings.providerTools.speechSynthesize": "语音合成",
    "settings.providerTools.speechTranscribe": "语音转写",
    "settings.providerTools.videoGenerate": "视频生成",
    "settings.providerTools.videoEdit": "视频编辑",
    "settings.providerTools.webSearch": "网页搜索",
    "settings.providerTools.webExtract": "网页抓取",
    "settings.providerTools.loading": "正在读取能力目标…",
    "settings.providerTools.enabled": "已启用",
    "settings.providerTools.default": "默认目标",
    "settings.providerTools.setDefault": "设为默认",
    "settings.providerTools.noTargets": "当前没有支持此能力的模型。",
    "settings.providerTools.change": "更改目标",
    "settings.providerTools.noSelection": "该能力尚未选择目标。",
    "settings.providerTools.configure": "配置目标",
    "settings.providerTools.dialogTitle": "配置能力目标",
    "settings.providerTools.dialogDescription": "左侧选择供应商，右侧为当前能力选择唯一目标。",
    "settings.providerTools.providers": "供应商",
    "settings.providerTools.targets": "目标列表",
    "settings.providerTools.systemDefault": "系统默认配置",
    "settings.providerTools.systemDefaultHint": "能力开关、默认模型与默认账号相互独立；新会话创建时复制此配置。",
    "settings.providerTools.model": "模型",
    "settings.providerTools.account": "账号",
    "settings.providerTools.selectModel": "请选择模型",
    "settings.providerTools.selectAccount": "请选择账号",
    "composer.capability.conflict.tooltip": "能力设置需要重新确认",
    "composer.capability.conflict.banner": "已保存的模型或账号不再可用，请重新选择并保存后再发送。",
    "composer.capability.conflict.missingSelection": "此能力尚未选择模型和账号。",
    "composer.capability.conflict.targetUnavailable": "已保存的模型或账号不再可用。",
    "settings.providerTools.disable": "不启用",
    "settings.providerTools.cancel": "取消",
    "settings.providerTools.save": "保存",
    "settings.providerTools.test": "测试",
    "settings.providerTools.testSearchTitle": "测试网页搜索",
    "settings.providerTools.testExtractTitle": "测试网页抓取",
    "settings.providerTools.testSearchDescription": "使用当前凭据和目标执行一次真实搜索，不会修改默认目标。",
    "settings.providerTools.testExtractDescription": "使用当前凭据和目标执行一次真实网页抓取，不会修改默认目标。",
    "settings.providerTools.testQuery": "搜索内容",
    "settings.providerTools.testQueryPlaceholder": "输入要搜索的内容",
    "settings.providerTools.testMaxResults": "结果数量",
    "settings.providerTools.testIncludeDomains": "仅包含域名",
    "settings.providerTools.testExcludeDomains": "排除域名",
    "settings.providerTools.testDomainsPlaceholder": "每行一个域名",
    "settings.providerTools.testUrls": "网页地址",
    "settings.providerTools.testUrlsPlaceholder": "每行一个 HTTPS 地址",
    "settings.providerTools.testExtractQuery": "相关内容",
    "settings.providerTools.testChunksPerSource": "每页片段数",
    "settings.providerTools.testDepth": "抓取深度",
    "settings.providerTools.testDepthBasic": "基础",
    "settings.providerTools.testDepthAdvanced": "高级",
    "settings.providerTools.testFormat": "内容格式",
    "settings.providerTools.testFormatMarkdown": "Markdown",
    "settings.providerTools.testFormatText": "纯文本",
    "settings.providerTools.testTimeoutSeconds": "超时秒数",
    "settings.providerTools.testIncludeImages": "包含图片",
    "settings.providerTools.testIncludeFavicon": "包含站点图标",
    "settings.providerTools.testConsumesQuota": "该操作会调用真实供应商接口并消耗账号额度。",
    "settings.providerTools.testRunning": "测试中…",
    "settings.providerTools.testResults": "测试结果",
    "settings.providerTools.testNoResults": "供应商没有返回可显示结果。",
    "settings.providerTools.testSucceeded": "成功",
    "settings.providerTools.testFailed": "失败",
    "settings.providerTools.testProviderRequestId": "供应商请求 ID",
    "toolPresentation.providerText.title": "AI 分析",
    "toolPresentation.providerText.empty": "供应商未返回文本结果。",
    "toolPresentation.providerSearch.title": "网页搜索",
    "toolPresentation.providerSearch.results": "条结果",
    "toolPresentation.providerSearch.empty": "供应商未返回搜索结果。",
    "toolPresentation.providerExtract.title": "网页提取",
    "toolPresentation.providerExtract.results": "个页面",
    "toolPresentation.providerExtract.failures": "个失败",
    "toolPresentation.providerExtract.images": "张图片",
    "toolPresentation.providerExtract.empty": "供应商未返回提取结果。",
    "toolPresentation.providerStructured.embedding": "向量结果",
    "toolPresentation.providerStructured.rerank": "重排结果",
    "toolPresentation.providerStructured.composite": "复合结果",
    "toolPresentation.providerStructured.items": "项",
    "settings.modelOverrides.disabled": "停用",
    "settings.modelOverrides.cancel": "取消",
    "settings.modelOverrides.save": "保存更改",
    "settings.modelOverrides.saving": "保存中…",
    "settings.modelOverrides.validation.safeInteger": "必须是安全整数",
    "settings.modelOverrides.validation.positiveInteger": "必须是正整数",
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
    "gitCommitDialog.reviewSettings": "审查设置",
    "gitCommitDialog.monitoringEnabled": "监听中",
    "gitCommitDialog.monitoringDisabled": "已关闭",
    "gitCommitDialog.monitoringManager": "GIT设置",
    "gitCommitDialog.monitoringManagerDescription": "选择需要监听的仓库，并调整显示顺序。第一个启用仓库固定展开。",
    "gitCommitDialog.moveRepositoryUp": "上移仓库",
    "gitCommitDialog.moveRepositoryDown": "下移仓库",
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
    "conversation.submission.creating": "Creating conversation",
    "conversation.submission.preparing": "Preparing workspace",
    "conversation.submission.uploading": "Uploading media",
    "conversation.submission.uploadProgress": "Media upload progress",
    "conversation.submission.cancelling": "Cancelling upload",
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
    "conversation.systemError.title": "System error",
    "conversation.systemError.status": "Failed",
    "conversation.systemError.message": "Error message",
    "conversation.systemError.provider": "Provider",
    "conversation.systemError.category": "Category",
    "conversation.systemError.stage": "Stage",
    "conversation.systemError.dispatchOutcome": "Dispatch outcome",
    "conversation.systemError.semanticOutput": "Semantic output observed",
    "conversation.systemError.retryable": "Retryable",
    "conversation.systemError.httpStatus": "HTTP status",
    "conversation.systemError.subcategory": "Subcategory",
    "conversation.systemError.upstreamCode": "Upstream code",
    "conversation.systemError.retryAfter": "Retry after",
    "conversation.systemError.true": "Yes",
    "conversation.systemError.false": "No",
    "conversation.systemError.raw": "View raw error",
    "conversation.systemError.retry": "Retry",
    "conversation.systemError.retrying": "Retrying",
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
    "contextCompaction.progress.prepareRemoteTooling": "Rebuilding the current session's tool, plugin, and MCP snapshot for remote compaction.",
    "contextCompaction.progress.remoteCompactionCheck": "Preparing remote compaction and waiting for the provider; upstream latency may vary, and the task is still running.",
    "contextCompaction.progress.remoteCompactionComplete": "Remote provider compaction completed; applying the new context checkpoint.",
    "contextCompaction.progress.remoteCompactionLocalFallback": "Remote compaction was unavailable or did not complete safely; switching to local compaction while the task continues.",
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
    "composer.capability.sessionRequired": "Send the first message to create the conversation before saving session capability choices.",
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
    "composer.modelPicker.filter": "Filter by model name or ID",
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
    "composer.modelPicker.currentAccount": "Current account: {account}",
    "composer.modelPicker.selectAccount": "Select account for {model}",
    "composer.aria.account": "Select model account",
    "composer.accountCard.refresh": "Refresh account usage",
    "composer.accountCard.provider": "Provider",
    "composer.accountCard.serviceEntry": "Service entry",
    "composer.accountCard.credentialType": "Credential type",
    "composer.accountCard.protocolType": "Protocol type",
    "composer.accountCard.loading": "Loading account information...",
    "composer.accountCard.metadataUnavailable": "Account information is unavailable",
    "composer.accountCard.plan": "Plan",
    "composer.accountCard.accountType": "Account type",
    "composer.accountCard.nonPlan": "Non-plan",
    "composer.accountCard.balance": "Balance",
    "composer.accountCard.usageUnsupported": "Usage lookup is unsupported",
    "composer.accountCard.managementKeyRequired": "Add an optional management key to query balance",
    "composer.accountCard.usageNotQueried": "Usage has not been queried",
    "composer.accountCard.usageUnavailable": "Account usage is temporarily unavailable",
    "composer.accountCard.usageInvalid": "The account usage response is invalid; refresh it again",
    "composer.accountCard.noQuotaWindows": "This provider does not expose plan quota windows",
    "composer.accountCard.remaining": "Remaining",
    "composer.accountCard.weeklyRemaining": "Weekly",
    "composer.accountCard.kimiWeeklyUsage": "Weekly usage",
    "composer.accountCard.kimiFiveHourUsage": "5-hour usage",
    "composer.accountCard.kimiLimit": "Limit",
    "composer.accountCard.reset": "Reset",
    "composer.accountCard.notIncluded": "Not included",
    "composer.accountCard.unlimited": "Unlimited",
    "composer.accountCard.tavilyPlanQuota": "Plan",
    "composer.accountCard.used": "Used",
    "composer.accountCard.general": "General",
    "composer.accountCard.video": "Video",
    "composer.accountCard.billingTokenPlan": "Token plan",
    "composer.accountCard.billingPayAsYouGo": "Pay as you go",
    "composer.accountCard.billingProviderManaged": "Provider managed",
    "composer.secondaryRoute.select": "Select secondary provider",
    "composer.secondaryRoute.search": "Search secondary providers",
    "composer.secondaryRoute.refresh": "Refresh secondary providers",
    "composer.secondaryRoute.refreshed": "Secondary provider list refreshed.",
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
    "conversationActions.previewCannotRetry": "Real conversations cannot be retried in browser preview mode.",
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
    "projectSettings.codebaseMemory.availability": "Published index",
    "projectSettings.codebaseMemory.availability.ready": "Available",
    "projectSettings.codebaseMemory.availability.degraded": "Available with warnings",
    "projectSettings.codebaseMemory.availability.unavailable": "Not available yet",
    "projectSettings.codebaseMemory.availability.offline": "Offline cache",
    "projectSettings.codebaseMemory.availability.unknown": "Not checked",
    "projectSettings.codebaseMemory.lastIndexed": "Last successful index",
    "projectSettings.codebaseMemory.revision": "Index revision",
    "projectSettings.codebaseMemory.watcher": "Incremental watcher",
    "projectSettings.codebaseMemory.registered": "Registered",
    "projectSettings.codebaseMemory.notRegistered": "Not registered",
    "projectSettings.codebaseMemory.graphSize": "Current graph",
    "projectSettings.codebaseMemory.graphSizeValue": "{nodes} nodes · {edges} edges",
    "projectSettings.codebaseMemory.unavailable": "Unavailable",
    "projectSettings.codebaseMemory.reindex": "Reindex project",
    "projectSettings.codebaseMemory.refresh": "Refresh index status",
    "projectSettings.codebaseMemory.update": "Sync index",
    "projectSettings.codebaseMemory.rebuild": "Full rebuild",
    "projectSettings.codebaseMemory.rebuildTitle": "Fully rebuild the project index?",
    "projectSettings.codebaseMemory.rebuildDescription": "A full rebuild rescans the project in an isolated database and atomically replaces the current index only after success. The currently published index remains queryable during the rebuild.",
    "projectSettings.codebaseMemory.confirmRebuild": "Confirm full rebuild",
    "projectSettings.codebaseMemory.cancel": "Cancel",
    "projectSettings.codebaseMemory.lastJob": "Latest job",
    "projectSettings.codebaseMemory.job.availableDuringUpdate": "A background job does not make the already published index unavailable.",
    "projectSettings.codebaseMemory.job.mode.update": "Index sync",
    "projectSettings.codebaseMemory.job.mode.rebuild": "Full rebuild",
    "projectSettings.codebaseMemory.job.phase.queued": "Waiting to start",
    "projectSettings.codebaseMemory.job.phase.discovering": "Scanning files",
    "projectSettings.codebaseMemory.job.phase.extracting": "Extracting structure",
    "projectSettings.codebaseMemory.job.phase.resolving": "Resolving relationships",
    "projectSettings.codebaseMemory.job.phase.persisting": "Writing index",
    "projectSettings.codebaseMemory.job.phase.publishing": "Publishing index",
    "projectSettings.codebaseMemory.job.phase.finalizing": "Finalizing",
    "projectSettings.codebaseMemory.job.state.queued": "Queued",
    "projectSettings.codebaseMemory.job.state.running": "Running",
    "projectSettings.codebaseMemory.job.state.succeeded": "Completed",
    "projectSettings.codebaseMemory.job.state.failed": "Failed",
    "projectSettings.codebaseMemory.job.state.cancelled": "Cancelled",
    "projectSettings.codebaseMemory.enabledSaved": "Project Codebase Memory enabled.",
    "projectSettings.codebaseMemory.disabledSaved": "Project Codebase Memory disabled; existing indexes were retained.",
    "projectSettings.codebaseMemory.reindexAccepted": "Project reindex request submitted.",
    "projectSettings.codebaseMemory.updateAccepted": "Index sync job submitted.",
    "projectSettings.codebaseMemory.rebuildAccepted": "Full rebuild job submitted.",
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
    "vmm.experience.title": "Memory service",
    "vmm.experience.subtitle": "Configure, start, and manage Vulcan Memory Mesh long-term memory.",
    "vmm.experience.quickSetup.title": "Long-term memory is not enabled",
    "vmm.experience.quickSetup.description": "Enable locally managed memory with recommended settings. Vulcan Code checks the runtime, model configuration, and data safety before starting.",
    "vmm.experience.quickSetup.action": "Start quick setup",
    "vmm.experience.quickSetup.running": "Saving configuration and starting the memory service…",
    "vmm.experience.status": "Service status",
    "vmm.experience.deployment": "Deployment",
    "vmm.experience.setup": "Setup",
    "vmm.experience.lifecycle": "Process",
    "vmm.experience.health": "Availability",
    "vmm.experience.prerequisites": "Configuration checks",
    "vmm.experience.capabilities": "Capabilities",
    "vmm.experience.activity": "Recent activity",
    "vmm.experience.pendingWritebacks": "Pending writes",
    "vmm.experience.completedTurns": "Processed conversations",
    "vmm.experience.lastProfileUpdate": "Last profile update",
    "vmm.experience.currentProject": "Default memory project",
    "vmm.experience.currentUser": "Default memory user",
    "vmm.experience.noProject": "No default memory project selected",
    "vmm.experience.noUser": "No default memory user selected",
    "vmm.experience.refresh": "Refresh status",
    "vmm.experience.start": "Start service",
    "vmm.experience.restart": "Restart",
    "vmm.experience.stop": "Stop service",
    "vmm.experience.openAdvanced": "Open advanced settings",
    "memory.dataSpace.unknown.title": "Memory data has not been checked",
    "memory.dataSpace.unknown.description": "Data compatibility will be checked automatically when the local memory service starts.",
    "memory.dataSpace.empty.title": "Memory data space is empty",
    "memory.dataSpace.empty.description": "A new memory data space will be created safely on first start.",
    "memory.dataSpace.ready.title": "Memory data is ready",
    "memory.dataSpace.ready.description": "Existing memory data matches the current embedding model configuration.",
    "memory.dataSpace.unmarkedHistory.title": "Historical memory data detected",
    "memory.dataSpace.unmarkedHistory.description": "The existing data has no trusted vector-space marker. Back up and rebuild the index before continuing.",
    "memory.dataSpace.fingerprintChanged.title": "Embedding configuration changed",
    "memory.dataSpace.fingerprintChanged.description": "Historical vectors do not match the current model or dimensions. Back up and rebuild the index first.",
    "memory.dataSpace.markerCorrupt.title": "Memory data marker is damaged",
    "memory.dataSpace.markerCorrupt.description": "Existing memory will not be overwritten. Inspect the data and choose a safe recovery path.",
    "memory.dataSpace.markerVersionUnsupported.title": "Memory data requires migration",
    "memory.dataSpace.markerVersionUnsupported.description": "The existing data uses an unsupported version and must be upgraded through the formal migration flow.",
    "memory.dataSpace.schemaMigrationRequired.title": "Memory storage requires an upgrade",
    "memory.dataSpace.schemaMigrationRequired.description": "The storage schema differs from the current version and must be migrated first.",
    "memory.dataSpace.rebuildRunning.title": "Rebuilding the memory index",
    "memory.dataSpace.rebuildRunning.description": "The vector index is being rebuilt safely in the background. Keep the app open until it finishes.",
    "memory.dataSpace.handleLater": "Handle later",
    "memory.dataSpace.repair": "View recovery options",
    "memory.identity.title": "Users and projects",
    "memory.identity.description": "Manage durable memory users and project mappings. Internal identifiers remain hidden and are used only for routing.",
    "memory.identity.refresh": "Refresh list",
    "memory.identity.users": "Users",
    "memory.identity.projects": "Projects",
    "memory.identity.addUser": "Add user",
    "memory.identity.addProject": "Add project",
    "memory.identity.userName": "User name",
    "memory.identity.teamName": "Team",
    "memory.identity.spaceName": "Space",
    "memory.identity.projectName": "Project",
    "memory.identity.projectPathStructure": "VMM projects use a three-level Team / Space / Project path. Each level is an independent memory scope.",
    "memory.identity.projectPathPreview": "Full path",
    "memory.identity.projectPlaceholder": "For example: vulcan-code",
    "memory.identity.create": "Create",
    "memory.identity.migrate": "Migrate",
    "memory.identity.delete": "Delete",
    "memory.identity.default": "Default",
    "memory.identity.setDefault": "Set as default",
    "memory.identity.protected": "System default",
    "memory.identity.emptyUsers": "No durable memory users have been created.",
    "memory.identity.emptyProjects": "No durable memory projects have been created.",
    "memory.identity.confirmDeleteTitle": "Confirm deletion",
    "memory.identity.confirmDeleteDescription": "This removes associated durable memory and profile data. The service checks the impact before the final deletion.",
    "memory.identity.typeName": "Type “{name}” to confirm",
    "memory.identity.checking": "Checking impact…",
    "memory.identity.migrateTitle": "Migrate project",
    "memory.identity.migrateDescription": "Safely migrate source project memory into an existing destination project. The service runs a preflight check first.",
    "memory.identity.targetProjectName": "Destination project",
    "memory.common.cancel": "Cancel",
    "memory.common.close": "Close",
    "memory.common.save": "Save",
    "memory.common.apply": "Apply",
    "memory.common.loading": "Working…",
    "memory.rules.title": "Memory rules",
    "memory.rules.description": "Adjust the rules first, then apply them once to all projects or one selected project.",
    "memory.rules.enable": "Enable durable memory",
    "memory.rules.enableDescription": "Allow Vulcan Code to save and recall durable memory for conversations.",
    "memory.rules.recall": "Automatic recall",
    "memory.rules.recallDescription": "Retrieve memory related to the current task before each turn.",
    "memory.rules.organize": "Automatic organization",
    "memory.rules.organizeDescription": "Organize and write back valuable information after a conversation completes.",
    "memory.rules.aiTools": "Let AI manage memory",
    "memory.rules.aiToolsDescription": "Expose memory search, write, and delete tools to the current model.",
    "memory.rules.recallTurns": "Implicit memory retention turns",
    "memory.rules.profileRefresh": "Profile refresh interval",
    "memory.rules.profileLimit": "Profile item limit",
    "memory.rules.applySettings": "Apply settings",
    "memory.rules.applyTitle": "Choose where to apply",
    "memory.rules.applyDescription": "This change writes to one explicit scope and never overwrites unselected projects in bulk.",
    "memory.rules.allProjects": "All projects (global default)",
    "memory.rules.oneProject": "One selected project",
    "memory.rules.selectProject": "Select a project",
    "memory.rules.searchProject": "Search projects…",
    "memory.rules.noProject": "No projects available",
    "memory.rules.applied": "Settings applied.",
    "memory.content.title": "Memory content",
    "memory.content.description": "Use natural language to search, inspect, and maintain durable memory for the current user and project.",
    "memory.content.sessionRequiredTitle": "A conversation is required to add memory",
    "memory.content.sessionRequired": "Open a specific conversation in the workbench before adding memory here. Searching and inspecting existing memory remain available.",
    "memory.content.search": "Search memory",
    "memory.content.searchPlaceholder": "For example: Which database did we choose last time?",
    "memory.content.add": "Add memory",
    "memory.content.empty": "Enter a question to search durable memory.",
    "memory.content.noResults": "No related memory was found.",
    "memory.content.details": "View details",
    "memory.content.category": "Category",
    "memory.content.createdAt": "Created",
    "memory.content.deleteTitle": "Delete this memory?",
    "memory.content.deleteDescription": "It will no longer be recalled automatically. The corresponding vector index is removed as well.",
    "memory.content.deleteReason": "Deletion reason (optional)",
    "memory.content.addTitle": "Add durable memory",
    "memory.content.summary": "Short summary",
    "memory.content.fullDetails": "Full details",
    "memory.content.write": "Save memory",
    "memory.content.sourceConversation": "Source conversation",
    "memory.data.title": "Memory data",
    "memory.data.description": "Inspect long-term memory sources by session and turn, then archive, recycle, or restore data through an impact preview.",
    "memory.data.managementReady": "Management service ready",
    "memory.data.managementUnavailable": "Management service unavailable",
    "memory.data.invalidResponse": "The VMM management service returned data that does not match its contract.",
    "memory.data.activeRuntime": "Running; actions unavailable",
    "memory.data.sessions": "Sessions",
    "memory.data.recycleBin": "Recycle bin",
    "memory.data.memoryEntries": "Memory entries",
    "memory.data.searchPlaceholder": "Search session titles or content excerpts",
    "memory.data.statusAll": "All statuses",
    "memory.data.statusActive": "Active",
    "memory.data.statusArchived": "Archived",
    "memory.data.statusRecycled": "Recycled",
    "memory.data.statusForgotten": "Permanently cleared",
    "memory.data.allStatuses": "All extraction states",
    "memory.data.extractionPending": "Pending extraction",
    "memory.data.extractionCompleted": "Extracted",
    "memory.data.extractionFailed": "Extraction failed",
    "memory.data.allUsers": "All memory users",
    "memory.data.allProjects": "All memory projects",
    "memory.data.createdFrom": "Created from",
    "memory.data.createdTo": "Created to",
    "memory.data.refresh": "Refresh",
    "memory.data.resetFilters": "Reset filters",
    "memory.data.fullTextIndex": "Full-text index",
    "memory.data.selected": "{count} selected",
    "memory.data.mixedLifecycle": "The selected sessions have different archive states. Archive or unarchive one state at a time.",
    "memory.data.archive": "Archive",
    "memory.data.unarchive": "Unarchive",
    "memory.data.recycle": "Move to recycle bin",
    "memory.data.restartMemory": "Restart memory",
    "memory.data.noSessions": "No memory sessions match these filters.",
    "memory.data.loadMore": "Load more",
    "memory.data.viewTurns": "View turns",
    "memory.data.backToSessions": "Back to sessions",
    "memory.data.turns": "Session turns",
    "memory.data.noTurns": "This session has no visible turns.",
    "memory.data.searchTurnPlaceholder": "Search user questions or assistant responses",
    "memory.data.details": "View details",
    "memory.data.turnDetails": "Turn details",
    "memory.data.userContent": "User content",
    "memory.data.assistantContent": "Assistant content",
    "memory.data.loading": "Loading…",
    "memory.data.impactTitle": "Confirm data impact",
    "memory.data.impactDescription": "Review the sessions, turns, and derived data affected before continuing.",
    "memory.data.confirmAction": "Confirm action",
    "memory.data.cancel": "Cancel",
    "memory.data.turnCount": "Turns",
    "memory.data.memoryCount": "Memories",
    "memory.data.profileCount": "Profiles",
    "memory.data.vectorCount": "Vectors",
    "memory.data.pendingCount": "Pending",
    "memory.data.hotCount": "Hot window",
    "memory.data.protectedCount": "Protected",
    "memory.data.recycleEmpty": "There is no manageable data in the recycle bin.",
    "memory.data.restore": "Restore",
    "memory.data.purge": "Delete permanently",
    "memory.data.purgeTitle": "Permanently delete recycled data",
    "memory.data.purgeDescription": "This cannot be undone. Enter the required phrase to continue.",
    "memory.data.purgePlaceholder": "永久删除",
    "memory.data.restorable": "Restorable",
    "memory.data.systemBatch": "System archive; restore unavailable",
    "memory.data.updatedAt": "Updated {time}",
    "memory.profile.title": "Profile",
    "memory.profile.description": "Choose a memory user or memory project, review the durable preferences and constraints organized by VMM, and correct them with natural language.",
    "memory.profile.target": "Profile target",
    "memory.profile.user": "User profile",
    "memory.profile.project": "Project profile",
    "memory.profile.selectUser": "Select memory user",
    "memory.profile.selectProject": "Select memory project",
    "memory.profile.searchUser": "Search memory users…",
    "memory.profile.searchProject": "Search memory projects…",
    "memory.profile.noUser": "No memory users available",
    "memory.profile.noProject": "No memory projects available",
    "memory.profile.refresh": "Refresh profile",
    "memory.profile.empty": "The current target has no profile items yet.",
    "memory.profile.instruction": "Profile correction instruction",
    "memory.profile.instructionPlaceholder": "For example: Do not treat the old database approach as the current decision.",
    "memory.profile.applyInstruction": "Apply instruction",
    "memory.profile.preview": "Final profile preview",
    "memory.profile.previewDescription": "This is the complete profile text supplied to the memory service. Open it only for review or troubleshooting.",
    "memory.profile.priority": "Priority",
    "memory.profile.level": "Level",
    "memory.advanced.title": "Advanced settings",
    "memory.advanced.description": "Choose local hosting or an external service, and inspect connections, data recovery, and diagnostics when needed.",
    "memory.advanced.deployment": "Deployment",
    "memory.advanced.managed": "Locally managed (recommended)",
    "memory.advanced.managedDescription": "Vulcan Code starts, stops, and protects the local memory service automatically.",
    "memory.advanced.external": "Connect an external VMM",
    "memory.advanced.externalDescription": "Connect to a VMM service operated independently by you or your team.",
    "memory.advanced.connection": "External service connection",
    "memory.advanced.endpoint": "gRPC endpoint",
    "memory.advanced.apiKey": "Access key",
    "memory.advanced.managementEndpoint": "Management API endpoint",
    "memory.advanced.managementToken": "Management token",
    "memory.advanced.testManagement": "Save and test management connection",
    "memory.advanced.managementTimeout": "Management timeout (ms)",
    "memory.advanced.runtime": "Local runtime",
    "memory.advanced.autoStart": "Start with Vulcan Code",
    "memory.advanced.manualStart": "Manual start only",
    "memory.advanced.transport": "Advanced transport",
    "memory.advanced.diagnostics": "Technical diagnostics",
    "memory.advanced.diagnosticRuntime": "Runtime state",
    "memory.advanced.diagnosticGrpc": "Service endpoint",
    "memory.advanced.diagnosticController": "Storage controller",
    "memory.advanced.diagnosticData": "Data directory",
    "memory.advanced.diagnosticLog": "Runtime log",
    "memory.advanced.data": "Memory data",
    "memory.advanced.save": "Save advanced settings",
    "memory.advanced.backupRebuild": "Back up and rebuild",
    "memory.advanced.maintenanceUnavailable": "This version does not yet expose a safe maintenance entry. Existing data will not be modified.",
    "memory.maintenance.confirmTitle": "Safely process historical memory",
    "memory.maintenance.confirmDescription": "Vulcan Code will create a complete backup first, then rebuild the index with the current embedding model. Original data is not deleted directly.",
    "memory.maintenance.backupFirst": "Back up before rebuilding",
    "memory.maintenance.backupFirstDescription": "Existing vectors are not modified before backup completes. A failed rebuild keeps both original data and the completed backup.",
    "memory.maintenance.later": "Handle later",
    "memory.maintenance.progressTitle": "Processing memory data",
    "memory.maintenance.progressDescription": "Leaving this page does not interrupt the operation. Reopen Advanced settings to see progress.",
    "memory.maintenance.progressCount": "Processed {processed} / {total}",
    "memory.maintenance.cancel": "Cancel maintenance",
    "memory.maintenance.completed": "Data processing complete",
    "memory.maintenance.failed": "Data processing failed",
    "memory.maintenance.cancelled": "Data processing cancelled",
    "memory.maintenance.startMemory": "Start memory service",
    "memory.maintenance.startFailed": "The memory service has not started",
    "memory.maintenance.startFailedDescription": "Backup and rebuild completed. Keep this dialog open, follow the page guidance to check runtime files, model configuration, or ports, then retry.",
    "vmm.experience.deployment.disabled": "Disabled",
    "vmm.experience.deployment.managed": "Managed locally",
    "vmm.experience.deployment.external": "External service",
    "vmm.experience.setup.notConfigured": "Not configured",
    "vmm.experience.setup.checking": "Checking",
    "vmm.experience.setup.ready": "Ready",
    "vmm.experience.setup.incomplete": "Needs attention",
    "vmm.experience.setup.blocked": "Blocked",
    "vmm.experience.lifecycle.stopped": "Stopped",
    "vmm.experience.lifecycle.starting": "Starting",
    "vmm.experience.lifecycle.running": "Running",
    "vmm.experience.lifecycle.stopping": "Stopping",
    "vmm.experience.lifecycle.restarting": "Restarting",
    "vmm.experience.lifecycle.crashed": "Crashed",
    "vmm.experience.lifecycle.waitingRetry": "Waiting to retry",
    "vmm.experience.lifecycle.failed": "Start failed",
    "vmm.experience.health.unknown": "Not checked",
    "vmm.experience.health.healthy": "Healthy",
    "vmm.experience.health.degraded": "Partially available",
    "vmm.experience.health.unavailable": "Unavailable",
    "vmm.experience.state.available": "Available",
    "vmm.experience.state.unavailable": "Unavailable",
    "vmm.experience.prerequisite.memory.title": "Enable long-term memory",
    "vmm.experience.prerequisite.memory.description": "Vulcan Code can recall and write long-term memory after quick setup.",
    "vmm.experience.prerequisite.transport.title": "Configure the memory service",
    "vmm.experience.prerequisite.transport.description": "Choose a locally managed or external service and complete its connection settings.",
    "vmm.experience.prerequisite.user.title": "Create a memory user",
    "vmm.experience.prerequisite.user.description": "Select or create the identity used for durable memory.",
    "vmm.experience.prerequisite.project.title": "Link the current project",
    "vmm.experience.prerequisite.project.description": "Link the current workspace to a durable memory project.",
    "vmm.experience.prerequisite.runtime.title": "Repair memory service startup",
    "vmm.experience.prerequisite.runtime.description": "Review the controlled error and retry. Existing memory data is never overwritten silently.",
    "vmm.experience.capability.recall": "Automatic recall before replies",
    "vmm.experience.capability.writeback": "Automatic maintenance after replies",
    "vmm.experience.capability.tools": "AI memory tools",
    "vmm.experience.capability.profile": "Profile instructions",
    "vmm.experience.capability.rerank": "Memory reranking",
    "vmm.setup.title": "Quick setup for long-term memory",
    "vmm.setup.description": "Use local managed mode to create a memory user and link the current project. Data safety is checked before any startup.",
    "vmm.setup.stepLabel": "Step {current} of {total}",
    "vmm.setup.previous": "Previous",
    "vmm.setup.next": "Next",
    "vmm.setup.mode.title": "Where the memory service runs",
    "vmm.setup.mode.description": "Managed locally is best for most users. Choose an external connection only when a VMM service already exists.",
    "vmm.setup.managed.title": "Managed locally",
    "vmm.setup.managed.description": "Vulcan Code starts, stops, and connects to the local VMM service.",
    "vmm.setup.external.title": "Connect to an external service",
    "vmm.setup.external.description": "Connect to a VMM run by you or your team without starting a local process.",
    "vmm.setup.inference.title": "Choose memory processing models",
    "vmm.setup.inference.description": "Select the model and account for memory extraction, organization, embeddings, and reranking. Each list is filtered to suitable models.",
    "vmm.setup.inference.required": "Configure and enable an embedding model plus the PreCheck and PostAction L1/L2 base models and accounts first. Memory is initialized only after every required route is valid; profile and reranking routes can be added later.",
    "vmm.setup.inference.openCredentials": "Configure model accounts",
    "vmm.setup.scope.title": "Choose where to enable memory",
    "vmm.setup.scope.description": "Define the memory identity, then choose where it takes effect. You can change this later under Memory rules.",
    "vmm.setup.scope.all_projects": "Enable for all projects now",
    "vmm.setup.scope.current_project": "Enable only for the current project",
    "vmm.setup.scope.currentProjectUnavailable": "No workspace project is available to link",
    "vmm.setup.scope.not_yet": "Configure only; do not enable yet",
    "vmm.setup.confirm.title": "Confirm setup",
    "vmm.setup.confirm.description": "Model and account choices were saved in the previous step. Confirm to save the service location and enablement scope, then create the memory space.",
    "vmm.setup.user": "Memory user name",
    "vmm.setup.user.placeholder": "For example: Local user",
    "vmm.setup.project": "Current project name",
    "vmm.setup.project.placeholder": "For example: vulcan-code",
    "vmm.setup.safety": "Existing data is never overwritten automatically. Setup pauses and offers explicit recovery options for historical data, vector-space changes, or corrupt markers.",
    "vmm.setup.cancel": "Cancel",
    "vmm.setup.submit": "Configure and start",
    "vmm.setup.running": "Checking, starting, and creating the memory space…",
    "vmm.setup.failed": "Quick setup is not complete. Follow the guidance below and try again.",
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
    "vmm.runtime.start": "Start",
    "vmm.runtime.restart": "Restart",
    "vmm.runtime.stop": "Stop",
    "vmm.runtime.openLog": "Open log",
    "vmm.runtime.mode": "Runtime mode",
    "vmm.runtime.mode.managed": "Managed by Vulcan Code",
    "vmm.runtime.mode.external": "External VMM",
    "vmm.runtime.phase.ready": "Managed VMM is running",
    "vmm.runtime.phase.starting": "Managed VMM is starting",
    "vmm.runtime.phase.stopped": "Managed VMM is stopped",
    "vmm.runtime.phase.failed": "Managed VMM failed to start",
    "vmm.runtime.phase.external": "Managed VMM is not running",
    "vmm.runtime.phase.crashed": "Managed VMM crashed",
    "vmm.runtime.phase.waiting_retry": "Managed VMM is waiting to retry",
    "vmm.runtime.phase.stopping": "Managed VMM is stopping",
    "vmm.runtime.grpcTarget": "Actual gRPC target",
    "vmm.runtime.controller": "Shared Controller",
    "vmm.runtime.controllerSpace": "VMM Space",
    "vmm.runtime.dataRoot": "Data root",
    "vmm.runtime.logPath": "Runtime log",
    "vmm.runtime.processCapability": "Process capability",
    "vmm.runtime.scopeCapability": "User/project reconciliation",
    "vmm.runtime.profileInstructionCapability": "Profile instruction",
    "vmm.runtime.capabilityReady": "Available",
    "vmm.runtime.capabilityUnavailable": "Unavailable",
    "vmm.result.title": "Action result",
    "vmm.result.emptySummary": "No VMM action has run yet.",
    "vmm.result.request": "Request",
    "vmm.result.response": "Response",
    "vmm.result.emptyHint": "Requests and responses appear after you run an action on the left.",
    "vmm.result.generic": "Result",
    "vmm.managed.behavior.identity": "Privacy, noise, and prompts",
    "vmm.managed.behavior.lifecycle": "Precheck and postaction",
    "vmm.managed.behavior.pipeline": "Retrieval and deduplication pipeline",
    "vmm.managed.behavior.retention": "Retention and recycling",
    "vmm.managed.piiLanguage": "PII rule language",
    "vmm.managed.noiseEnabled": "Enable noise filtering",
    "vmm.managed.noiseLanguage": "Noise rule language",
    "vmm.managed.noiseSemanticEnabled": "Enable semantic noise filtering",
    "vmm.managed.noiseSemanticThreshold": "Semantic noise threshold",
    "vmm.managed.promptLanguage": "Prompt bundle",
    "vmm.managed.precheckIntentTimeout": "Intent timeout",
    "vmm.managed.precheckTopK": "Precheck recall count",
    "vmm.managed.precheckSearchScope": "Precheck search scope",
    "vmm.managed.precheckSimilarityThreshold": "Precheck similarity threshold",
    "vmm.managed.scope.team": "Team",
    "vmm.managed.scope.space": "Space",
    "vmm.managed.scope.project": "Project",
    "vmm.managed.postactionInputMode": "Postaction input mode",
    "vmm.managed.inputMode.compat": "Compatible",
    "vmm.managed.inputMode.strict": "Strict",
    "vmm.managed.postactionTurnThreshold": "Session analysis turn threshold",
    "vmm.managed.postactionTokenThreshold": "Session analysis token threshold",
    "vmm.managed.postactionIdleTimeout": "Session analysis idle timeout",
    "vmm.managed.postactionHistoryTurns": "Session analysis history turns",
    "vmm.managed.postactionMaxInputTokens": "Session analysis max input tokens",
    "vmm.managed.postactionMaxQueueWorkers": "Postaction queue workers",
    "vmm.managed.pipelineMaxSearchKeywords": "Maximum search keywords",
    "vmm.managed.pipelineMinSimilarityScore": "Minimum recall similarity",
    "vmm.managed.pipelineReplaceMinSimilarityScore": "Minimum replacement similarity",
    "vmm.managed.pipelineHardDedupeThreshold": "Hard deduplication threshold",
    "vmm.managed.pipelineHardDedupePoolTopK": "Hard deduplication candidates",
    "vmm.managed.pipelineHybridEnabled": "Enable hybrid retrieval",
    "vmm.managed.pipelineLexicalTopK": "Lexical recall count",
    "vmm.managed.pipelineRrfK": "Reciprocal rank fusion constant",
    "vmm.managed.pipelineMmrEnabled": "Enable diversity filtering",
    "vmm.managed.pipelineMmrLambda": "Diversity balance",
    "vmm.managed.pipelineWeibullEnabled": "Enable Weibull decay",
    "vmm.managed.pipelineWeibullShape": "Weibull shape",
    "vmm.managed.pipelineWeibullScaleHours": "Weibull scale hours",
    "vmm.managed.pipelineWeibullMinMultiplier": "Minimum decay multiplier",
    "vmm.managed.pipelineWeibullReinforceWeight": "Reinforcement weight",
    "vmm.managed.pipelineWeibullCrossSessionBoost": "Cross-session boost",
    "vmm.managed.retentionEnabled": "Enable retention scans",
    "vmm.managed.retentionScanInterval": "Retention scan interval",
    "vmm.managed.retentionTurnKeepExtraTurns": "Extra hot-window turns",
    "vmm.managed.retentionSessionIdleAfter": "Session idle recycle duration",
    "vmm.managed.retentionTrashDuration": "Trash retention duration",
    "vmm.managed.retentionPriorityFloor": "Protected priority floor",
    "vmm.managed.retentionMemoryLevelFloor": "Protected memory-level floor",
    "vmm.managed.retentionSkipProtectedShared": "Skip protected shared memories",
    "vmm.settings.grpcTarget": "gRPC address",
    "vmm.settings.apiKey": "API Key",
    "vmm.settings.managedRuntime": "Manage local VMM with Vulcan Code",
    "vmm.settings.managedBinaryPath": "VMM executable",
    "vmm.settings.packagedBinary": "Leave empty to use the packaged sidecar",
    "vmm.settings.autoStartPolicy": "Startup policy",
    "vmm.settings.autoStart": "Start with Vulcan Code",
    "vmm.settings.manualStart": "Manual start",
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
    "settings.modelProviders": "Provider management",
    "settings.credentialManagement": "Credential management",
    "settings.providerCategory.official": "Official",
    "settings.providerCategory.cloudMaaS": "Cloud service",
    "settings.providerCategory.relay": "Relay",
    "settings.providerCategory.unclassified": "Unclassified",
    "settings.providerDescription.default": "This provider exposes registered model or AI capability endpoints.",
    "settings.providerDescription.minimax": "Official MiniMax models, API platform, and Token Plan services.",
    "settings.providerDescription.kimi": "Moonshot AI platform and Kimi Coding Plan services.",
    "settings.providerDescription.deepseek": "Official DeepSeek model API and account services.",
    "settings.providerDescription.alibaba": "Alibaba Cloud Model Studio, Coding Plan, and Token Plan services.",
    "settings.providerDescription.openrouter": "A unified model routing platform spanning multiple models and upstream endpoints.",
    "settings.providerDescription.apikeyfun": "A third-party model relay supporting multiple compatible protocols.",
    "settings.providerDescription.claudeapi": "A third-party model relay providing Anthropic-compatible access.",
    "settings.providerDescription.qiniu": "Model inference and compatible protocol services from Qiniu Cloud.",
    "settings.providerDescription.fennoai": "A third-party model relay supporting multiple compatible protocols.",
    "settings.providerDescription.code0": "A third-party model relay supporting OpenAI- and Anthropic-compatible protocols.",
    "settings.providerDescription.tavily": "Web search and content extraction services designed for AI applications.",
    "settings.providerDescription.openai": "Official OpenAI models and Codex account services.",
    "settings.providerDescription.anthropic": "Official Anthropic Claude models and Claude Code account services.",
    "settings.providerDescription.google": "Official Google Gemini API and Code Assist services.",
    "settings.providerSummary.serviceEntries": "{count} endpoints",
    "settings.providerSummary.protocols": "{count} supported protocols",
    "settings.providerSummary.serviceEntriesTitle": "Endpoints",
    "settings.providerSummary.protocolsTitle": "Supported protocols",
    "settings.serviceEntry.title.codingPlan": "Coding Plan",
    "settings.serviceEntry.title.tokenPlan": "Token Plan",
    "settings.serviceEntry.title.modelStudio": "Model Studio",
    "settings.serviceEntry.tag.personal": "Personal",
    "settings.serviceEntry.tag.team": "Team",
    "settings.serviceEntry.tag.china": "China",
    "settings.serviceEntry.tag.international": "International",
    "settings.providerOnboarding.notConfigured": "Credential required",
    "settings.providerOnboarding.description": "This provider stays visible; it joins model sync and execution only after a real credential is added.",
    "settings.providerOnboarding.serviceEntries": "available service entries",
    "settings.providerOnboarding.register": "Register",
    "settings.providerOnboarding.referralSignup": "Referral signup",
    "settings.providerOnboarding.addCredential": "Add credential",
    "settings.providerConnection.serviceOrigin": "Workspace-dedicated endpoint (optional)",
    "settings.providerConnection.serviceOriginDescription": "Paste the full OpenAI or Anthropic URL shown by the console, or leave it empty to use the shared endpoint. This setting applies only to this credential.",
    "settings.providerOnboarding.filteredEmpty": "No credential for this provider",
    "settings.providerOnboarding.filteredEmptyDescription": "Switch providers or use the header action to add a credential.",
    "settings.providerOnboarding.externalConfirmation": "Open the provider's external signup page?",
    "settings.providerOnboarding.externalOpenAnyway": "Open anyway",
    "settings.providerOnboarding.externalOpenFailed": "Failed to open provider signup page",
    "settings.providerPromotion.neutralDisclosure": "Neutral signup link with no Vulcan revenue attribution.",
    "settings.providerPromotion.referralDisclosure": "Registering through an invitation link may support the developers and help sustain Vulcan development; it does not affect model ranking or request routing.",
    "settings.providerPromotion.sponsoredDisclosure": "Sponsored entry; the relationship never changes model ranking or routing.",
    "settings.modelList": "Model list",
    "settings.providerRetry": "Request retry",
    "settings.aiVendors": "Capability vendors",
    "settings.aiWebSearch": "Web search",
    "settings.aiWebExtract": "Web extract",
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
    "settings.modelProviders.description": "Review all implemented providers and manage custom compatible providers.",
    "settings.credentialManagement.description": "Manage model accounts, protocols, usage, and default credentials.",
    "settings.modelList.description": "Review enabled models by provider and the active workbench model.",
    "settings.providerRetry.description": "Configure retry counts, wait intervals, and backoff rules for ordinary failures and 429 rate limits.",
    "settings.aiVendors.description": "Capability vendors and route targets shared by semantic AI tools.",
    "settings.aiWebSearch.description": "Web-search capability state and available targets.",
    "settings.aiWebExtract.description": "Web-extraction capability state and available targets.",
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
    "settings.modelOverrides.title": "Model overrides",
    "settings.modelOverrides.description": "Adjust runtime parameters for this provider, model, and protocol without changing account selection.",
    "settings.modelOverrides.limits": "Token limits",
    "settings.modelOverrides.limitsDescription": "Declared provider limits may only be narrowed. When no limit is declared, you may set one manually or leave it blank for provider defaults.",
    "settings.modelOverrides.contextLimit": "Context limit",
    "settings.modelOverrides.outputLimit": "Output token limit",
    "settings.modelOverrides.leaveBlank": "Leave blank to use the provider default",
    "settings.modelOverrides.providerMaximum": "Provider maximum",
    "settings.modelOverrides.providerNotDeclared": "No provider limit is declared; you may set an override manually.",
    "settings.modelOverrides.cache": "Prompt cache",
    "settings.modelOverrides.cacheDescription": "Use the provider behavior, or let Vulcan manage caching when the protocol supports it.",
    "settings.modelOverrides.cacheMode": "Cache policy",
    "settings.modelOverrides.providerDefault": "Provider default; do not inject controls",
    "settings.modelOverrides.managedExplicit": "Managed explicitly by Vulcan",
    "settings.modelOverrides.cacheTtl": "Cache TTL",
    "settings.modelOverrides.parameters": "Model parameters",
    "settings.modelOverrides.parametersDescription": "Only parameters registered for the current protocol are shown.",
    "settings.modelOverrides.noParameters": "This protocol has no additional model parameters.",
    "settings.modelOverrides.unsupportedParameter": "No visual editor is registered for this complex parameter",
    "settings.modelOverrides.enabled": "Enabled",
    "settings.customModels.title": "{provider} · Model management",
    "settings.customModels.description": "Model definitions are stored independently from credentials. Imports and manual edits write user overlays only and never modify synchronized sources.",
    "settings.customModels.pageModels": "Models",
    "settings.customModels.pageProviderSettings": "Provider settings",
    "settings.customModels.search": "Search model name or ID",
    "settings.customModels.filterAll": "All models",
    "settings.customModels.filterEnabled": "Enabled only",
    "settings.customModels.filterDisabled": "Disabled only",
    "settings.customModels.importDiscovered": "Import missing models from the live catalog",
    "settings.customModels.importTitle": "Choose models to import",
    "settings.customModels.importDescription": "Only selected catalog models are copied into the user overlay. The synchronized source remains unchanged.",
    "settings.customModels.importSelected": "Import {count} models",
    "settings.customModels.addManual": "Add a model manually",
    "settings.customModels.enableAll": "Enable all",
    "settings.customModels.disableAll": "Disable all",
    "settings.customModels.enabled": "Enabled",
    "settings.customModels.disabled": "Disabled",
    "settings.credentialModels.manage": "Manage models for this account",
    "settings.credentialModels.title": "Account models",
    "settings.credentialModels.description": "Controls only the models callable through this account. It does not modify the provider catalog or sibling accounts.",
    "settings.credentialModels.search": "Search model name or ID",
    "settings.credentialModels.filterAll": "All",
    "settings.credentialModels.filterEnabled": "Enabled",
    "settings.credentialModels.filterDisabled": "Disabled",
    "settings.credentialModels.enableAll": "Enable all",
    "settings.credentialModels.disableAll": "Disable all",
    "settings.credentialModels.sync": "Sync catalog",
    "settings.credentialModels.syncing": "Syncing",
    "settings.credentialModels.count": "{enabled} of {total} enabled",
    "settings.credentialModels.empty": "This account has no manageable models yet.",
    "settings.credentialModels.noResults": "No models match the current search and filter.",
    "settings.credentialModels.directoryFixed": "Built-in catalog",
    "settings.credentialModels.directoryDynamic": "Account catalog",
    "settings.credentialModels.directoryManual": "Manual catalog",
    "settings.credentialModels.directoryUnavailable": "No model catalog",
    "settings.credentialModels.disableConfirmTitle": "Disable every model on this account?",
    "settings.credentialModels.disableConfirmDescription": "This account will stop participating in model routing. New and loaded sessions will move to an available account. The provider catalog and sibling accounts are unchanged.",
    "settings.credentialModels.cancel": "Cancel",
    "settings.credentialModels.confirmDisable": "Disable all models",
    "settings.credentialModels.futureEnabled": "New models discovered by later syncs will be enabled automatically.",
    "settings.credentialModels.futureDisabled": "New models discovered by later syncs stay disabled until you enable them.",
    "settings.customModels.empty": "No models match the current filter.",
    "settings.customModels.noModelsToImport": "The live catalog has no new models to append.",
    "settings.customModels.removeConfirm": "Remove model “{name}” from this custom provider? The service will block deletion while a session locks the model or an internal route references it; switch those sessions or routes first.",
    "settings.customProviders.removeTitle": "Remove provider",
    "settings.customProviders.removeDescription": "Remove custom provider “{name}”? Confirmation permanently deletes the provider and all provider-owned configuration.",
    "settings.customProviders.removeModelsWarning": "{count} model customizations or route settings will also be deleted.",
    "settings.customProviders.removeModelsWarningGeneric": "The provider's model customizations, account model policies, and internal routes will also be deleted.",
    "settings.customProviders.removeCredentialsWarning": "After confirmation, the provider's {count} credentials will be deleted together with it.",
    "settings.customProviders.removeSessionsWarning": "If any sessions use this provider, their session model will become unavailable and automatically switch to the default or first available model.",
    "settings.customProviders.modelCount": "{count} models",
    "settings.customProviders.manageModels": "Manage models",
    "settings.customProviders.manageModelsFor": "Manage models for {provider}",
    "settings.customProviders.addTitle": "Add custom provider",
    "settings.customProviders.editTitle": "Edit custom provider",
    "settings.customProviders.dialogDescription": "One base URL binds one calling protocol. Confirm the endpoint first, then validate the model catalog with a real credential; new providers retain it as their first account.",
    "settings.customProviders.progressLabel": "Custom provider setup progress",
    "settings.customProviders.step.connection": "Endpoint",
    "settings.customProviders.step.credential": "Credential",
    "settings.customProviders.step.result": "Confirm",
    "settings.customProviders.displayName": "Display name",
    "settings.customProviders.baseUrl": "Base URL",
    "settings.customProviders.protocol": "Calling protocol",
    "settings.customProviders.codexCapabilities": "Codex Responses capabilities",
    "settings.customProviders.codexWebsocket": "Prefer WebSocket",
    "settings.customProviders.codexWebsocketHint": "Falls back to HTTP before semantic output when unsupported.",
    "settings.customProviders.codexRemoteCompaction": "Enable remote context compaction",
    "settings.customProviders.codexRemoteCompactionHint": "Uses /responses/compact and falls back to local compaction on safe failures.",
    "settings.customProviders.authentication": "Authentication header",
    "settings.customProviders.discoveryMode": "Model catalog",
    "settings.customProviders.discoveryStandard": "Standard /models",
    "settings.customProviders.discoveryCustomPath": "Custom same-origin path",
    "settings.customProviders.discoveryManual": "No catalog; manage manually",
    "settings.customProviders.discoveryPath": "Model catalog path",
    "settings.customProviders.protocolEndpoint": "Calling endpoint:",
    "settings.customProviders.modelsEndpoint": "Catalog endpoint:",
    "settings.customProviders.manualOnly": "No remote catalog request",
    "settings.customProviders.validationTitle": "Validate with a real credential",
    "settings.customProviders.validationEditDescription": "Choose one existing account for the initial probe. When protocol, endpoint, authentication, or discovery changes are saved, every account is validated with its own credential and migrated atomically. No account or secret is replaced, and display-name-only edits skip revalidation.",
    "settings.customProviders.validationCreateDescription": "After validation, this credential is securely saved as the provider's first account.",
    "settings.customProviders.validationCredentialName": "Validation credential name",
    "settings.customProviders.validationCredentialRequired": "Choose an existing account",
    "settings.customProviders.credentialFreeEditTitle": "No account; direct save is available",
    "settings.customProviders.credentialFreeEditDescription": "This provider has no credential, so remote validation is unavailable. You can still save its protocol, endpoint, authentication, and model-catalog settings; future accounts will be validated against this contract.",
    "settings.customProviders.firstCredentialName": "First credential name",
    "settings.customProviders.apiKey": "API Key",
    "settings.customProviders.apiKeyEditPlaceholder": "Used only for this validation",
    "settings.customProviders.apiKeyCreatePlaceholder": "Stored securely after validation",
    "settings.customProviders.credentialRequired": "Enter a credential name and a valid API Key.",
    "settings.customProviders.back": "Back",
    "settings.customProviders.next": "Next",
    "settings.customProviders.probing": "Validating…",
    "settings.customProviders.probe": "Validate and sync models",
    "settings.customProviders.validationResultDescription": "Validation finished. Choose the save action that matches the observed state.",
    "settings.customProviders.validationModelCount": "Models discovered",
    "settings.customProviders.validationPageCount": "Pages read",
    "settings.customProviders.validationCredential": "Credential checked",
    "settings.customProviders.validationObservedAt": "Validated at",
    "settings.customProviders.validationModelsTitle": "Discovered model confirmation",
    "settings.customProviders.validationEvidence": "Capability evidence {known}/{total}",
    "settings.customProviders.validationModelsHint": "Only the provider, credential, and user settings are saved; this remote model catalog remains process-memory only.",
    "settings.customProviders.manualFallbackHint": "This result can be retained for manual model management. The credential remains isolated as pending until one explicit model test succeeds.",
    "settings.customProviders.pendingSaveHint": "This is a rate limit or transient failure. You may explicitly retain the setup and retry synchronization later.",
    "settings.customProviders.saving": "Saving…",
    "settings.customProviders.saveVerified": "Save and continue",
    "settings.customProviders.saveManual": "Save and configure models manually",
    "settings.customProviders.savePending": "Save pending and retry later",
    "settings.customProviders.pendingConfirmTitle": "Save an unvalidated connection?",
    "settings.customProviders.pendingConfirmDescription": "The credential has not completed model-catalog validation. It will remain isolated after saving and cannot become the default account or enter conversation routing until the catalog is synchronized and one explicit model test succeeds.",
    "settings.customProviders.confirmPending": "Confirm pending save",
    "settings.customProviders.protocolOverridesConfirmTitle": "Delete old protocol model settings?",
    "settings.customProviders.protocolOverridesConfirmDescription": "After switching to {protocol}, {count} model setting(s) from another protocol cannot apply. Confirming deletes those incompatible settings while preserving existing target-protocol settings.",
    "settings.customProviders.protocolOverridesConfirmAction": "Delete and switch protocol",
    "settings.providerModels.description": "Choose which models this provider publishes to accounts and model lists. Switches save immediately; disabled models remain visible only in this management dialog.",
    "settings.providerModels.count": "Published {enabled} / {total}",
    "settings.providerModels.empty": "The current credentials have not synchronized any manageable models yet.",
    "settings.customProviders.displayNameRequired": "Enter a provider display name.",
    "settings.customProviders.baseUrlInvalid": "Enter a valid HTTP or HTTPS base URL.",
    "settings.customProviders.discoveryPathInvalid": "A custom catalog path must start with / and contain neither an origin nor parent traversal.",
    "settings.customProviders.discovery.standard": "Standard catalog",
    "settings.customProviders.discovery.custom_path": "Custom path",
    "settings.customProviders.discovery.manual": "Manual management",
    "settings.customModels.identityRequired": "Every model requires a model ID and display name.",
    "settings.customModels.duplicateId": "Model IDs must be unique. Rename the duplicate ID: {model}.",
    "settings.customModels.parametersInvalid": "Model parameters must be a JSON array.",
    "settings.customModels.basic": "Basics",
    "settings.customModels.basicHint": "Context defaults come from the common capability library. Manual models start at 200,000 and remain directly editable.",
    "settings.customModels.modelId": "Model ID",
    "settings.customModels.displayName": "Display name",
    "settings.customModels.contextLimit": "Context limit",
    "settings.customModels.outputLimit": "Output token limit (optional)",
    "settings.customModels.modalities": "Input and output",
    "settings.customModels.modalitiesHint": "Text is the base capability for conversation models. Declare only additional resource modalities here.",
    "settings.customModels.input": "Input resources",
    "settings.customModels.output": "Output resources",
    "settings.customModels.behavior": "Tools and reasoning",
    "settings.customModels.behaviorHint": "Declare only behavior the provider actually supports. Unknown is never treated as supported.",
    "settings.customModels.toolCalling": "Tool calling",
    "settings.customModels.parallelTools": "Parallel tool calling",
    "settings.customModels.reasoning": "Reasoning support",
    "settings.customModels.reasoningControl": "Reasoning control",
    "settings.customModels.supportUnknown": "Not declared",
    "settings.customModels.supportNative": "Native",
    "settings.customModels.supportEquivalent": "Equivalent",
    "settings.customModels.supportLossy": "Limited compatibility",
    "settings.customModels.supportUnsupported": "Unsupported",
    "settings.customModels.controlUnknown": "Not declared",
    "settings.customModels.controlFixedOff": "Always off",
    "settings.customModels.controlFixedOn": "Always on",
    "settings.customModels.controlToggle": "Toggle",
    "settings.customModels.controlEffortSelection": "Effort selection",
    "settings.customModels.controlProviderAdaptive": "Provider adaptive",
    "settings.customModels.reasoningEfforts": "Selectable reasoning efforts",
    "settings.customModels.effortNone": "Off",
    "settings.customModels.effortAdaptive": "Adaptive",
    "settings.customModels.effortMinimal": "Minimal",
    "settings.customModels.effortLow": "Low",
    "settings.customModels.effortMedium": "Medium",
    "settings.customModels.effortHigh": "High",
    "settings.customModels.effortExtraHigh": "Extra high",
    "settings.customModels.effortMax": "Maximum",
    "settings.customModels.effortUltra": "Ultra",
    "settings.customModels.advanced": "Advanced protocol mapping",
    "settings.customModels.advancedHint": "Fill this only after confirming provider request fields. Host-owned fields are rejected by the service to protect model, message, and tool payloads.",
    "settings.customModels.parameters": "Model parameter definitions (JSON array)",
    "settings.customModels.projection": "Request projection (JSON object)",
    "settings.customModels.projectionInvalid": "The request projection must be a JSON object.",
    "settings.customModels.remove": "Remove current model",
    "settings.customModels.selectHint": "Select a model on the left, import from the catalog, or add one manually.",
    "settings.customModels.save": "Save models",
    "settings.customModels.defaultsTitle": "Provider model settings",
    "settings.customModels.defaultsDescription": "Adjust the defaults for every conversation model from this provider; model settings can still override them.",
    "settings.customModels.defaultsEdit": "Edit defaults",
    "settings.customModels.contextSummary": "Context {count}",
    "settings.customModels.outputSummary": "Output {count}",
    "settings.customModels.textInputOutput": "Text input / text output",
    "settings.customModels.outputRecommendation": "Recommended output limit for this context: {count} tokens (capped at 32,768).",
    "settings.customModels.defaultsContextInvalid": "Context limit must be a positive integer.",
    "settings.customModels.defaultsOutputInvalid": "Output limit must be a positive integer no greater than the context limit.",
    "settings.customModels.defaultsReasoningInvalid": "Keep at least one reasoning level while reasoning is enabled.",
    "settings.customModels.defaultsCustomSource": "Provider settings",
    "settings.customModels.defaultsSystemSource": "System default",
    "settings.customModels.defaultsSave": "Save provider settings",
    "settings.customModels.fixedTextOutput": "Fixed text output",
    "settings.customModels.fixedTextOutputDescription": "Generic conversation providers support text output only; native system providers declare dedicated image and other output capabilities.",
    "settings.customModels.toolCallingDefaultDescription": "Tool calling is enabled by default and should be disabled only when the provider or model explicitly does not support it.",
    "settings.customModels.templateSearching": "Checking the local capability library for an exact model match…",
    "settings.customModels.templateUpdateAvailable": "A newer common capability template is available",
    "settings.customModels.templateUpdateDescription": "This model keeps its reviewed snapshot until you explicitly apply the newer revision.",
    "settings.customModels.templateCurrent": "Current snapshot",
    "settings.customModels.templateLatest": "Latest template",
    "settings.customModels.templateApplyUpdate": "Apply template update",
    "settings.customModels.templateApplied": "Common model template applied",
    "settings.customModels.templateRestore": "Choose again",
    "settings.customModels.templateNone": "No reliable common template",
    "settings.customModels.templateDefaultFallback": "The current provider defaults for context, output, and text capability will be used.",
    "settings.customModels.templateRejected": "Common templates were explicitly declined",
    "settings.customModels.templateReviewAgain": "Review again",
    "settings.customModels.templateSuggested": "Reusable common capability template found",
    "settings.customModels.templateApply": "Apply this template",
    "settings.customModels.templateKeepDefaults": "Keep provider defaults",
    "settings.customModels.templateMatch.exact": "Exact model ID match",
    "settings.customModels.templateMatch.alias": "Known alias match",
    "settings.customModels.templateMatch.provider_agnostic": "Matched without provider prefix",
    "settings.customModels.discoveryTitle": "Account catalog sync",
    "settings.customModels.discoverySummary": "{accounts} accounts expose {models} models; {failed} accounts need attention. Catalogs remain memory-only.",
    "settings.customModels.reviewTemplates": "Review capability hints",
    "settings.customModels.reviewTemplatesTitle": "Review model capability hints",
    "settings.customModels.reviewTemplatesDescription": "Only exact keys, explicit aliases, and unique exact matches without a provider prefix are shown. Unmatched models keep provider defaults.",
    "settings.customModels.reviewExact": "Safe for bulk apply",
    "settings.customModels.reviewMissing": "Uses provider defaults",
    "settings.customModels.reviewExactDescription": "Exact or deterministic matches",
    "settings.customModels.reviewMissingDescription": "No match: keep the 200K/32K text-only provider defaults",
    "settings.customModels.reviewComplete": "Every current model has been reviewed, or no template hint needs action.",
    "settings.customModels.applyAllExact": "Apply all deterministic hints ({count})",
    "settings.customModels.refreshAll": "Sync all accounts",
    "settings.customModels.refreshAccount": "Sync this account",
    "settings.customModels.lastSynchronized": "Last synchronized: {time}",
    "settings.customModels.notSynchronized": "Not synchronized",
    "settings.customModels.discoveryDetails": "Account sync details and diagnostics",
    "settings.customModels.discoveryModelCount": "{count} models",
    "settings.customModels.discoveryPageCount": "{count} pages",
    "settings.customModels.discoverySchema": "Parser contract: {revision}",
    "settings.customModels.discovery.notChecked": "Not checked",
    "settings.customModels.discovery.ready": "Catalog ready",
    "settings.customModels.discovery.empty": "Catalog empty",
    "settings.customModels.discovery.unsupported": "Catalog endpoint unsupported",
    "settings.customModels.discovery.authenticationFailed": "Credential validation failed",
    "settings.customModels.discovery.rateLimited": "Rate limited",
    "settings.customModels.discovery.temporarilyUnavailable": "Temporarily unavailable",
    "settings.customModels.discovery.invalidResponse": "Invalid response",
    "settings.customModels.discovery.stale": "Using prior in-memory result",
    "settings.customModels.discovery.failed": "Sync failed",
    "settings.customModels.sourceApi": "API catalog",
    "settings.customModels.sourceManual": "Manual declaration",
    "settings.customModels.sourceUserOverride": "Model override",
    "settings.customModels.sourceAccount": "Account catalog evidence",
    "settings.customModels.sourceTemplate": "Common template",
    "settings.customModels.sourceProviderDefault": "Provider defaults",
    "settings.customModels.backToList": "Back to model list",
    "settings.customModels.tabOverview": "Overview",
    "settings.customModels.tabAccounts": "Accounts",
    "settings.customModels.tabCapabilities": "Capabilities",
    "settings.customModels.tabAdvanced": "Advanced",
    "settings.customModels.accountReturned": "Catalog returned this model",
    "settings.customModels.accountNotReturned": "Catalog did not return this model",
    "settings.customModels.accountAvailable": "Currently available",
    "settings.customModels.accountUnavailable": "Currently unavailable",
    "settings.customModels.accountInherit": "Follow catalog",
    "settings.customModels.accountForceEnable": "Force allow",
    "settings.customModels.accountForceDisable": "Force disable",
    "settings.customModels.capabilityAccount": "Capability account",
    "settings.customModels.viewAccountCapabilities": "View capabilities",
    "settings.customModels.pendingValidation": "Pending model validation",
    "settings.customModels.pendingCannotDefault": "A pending account cannot be the default; complete a model test first",
    "settings.customModels.validateModel": "Test model",
    "settings.customModels.validateModelTitle": "Send one real model test?",
    "settings.customModels.validateModelDescription": "A minimal text request will be sent to “{model}” through account “{account}”. This may incur a small charge. A pending account becomes eligible for conversations only after visible text is received.",
    "settings.customModels.confirmValidation": "Confirm and test",
    "settings.customModels.validationComplete": "Model test succeeded",
    "settings.customModels.validationSucceeded": "Account “{account}” passed validation. Model response: {response}",
    "settings.customModels.noAccounts": "No account is available",
    "settings.customModels.noAccountsDescription": "Add and validate a credential before reviewing account-specific capabilities and availability.",
    "settings.customModels.saveCapabilityOverrides": "Save capability overrides",
    "settings.customModels.changeSummaryTitle": "Confirm change summary",
    "settings.customModels.changeSummaryDescription": "Review the settings that will be saved for model “{model}”.",
    "settings.customModels.confirmChanges": "Confirm and save",
    "settings.customModels.summaryModel": "Model",
    "settings.customModels.summaryAccount": "Account",
    "settings.customModels.summaryAvailability": "Account availability",
    "settings.customModels.summaryRoutingState": "Routing state",
    "settings.customModels.summaryCapabilityOverrides": "Capability overrides",
    "settings.customModels.summaryRemoveOverrides": "Remove every user capability override and restore source-priority resolution",
    "settings.customModels.summaryTemplate": "Capability source",
    "settings.customModels.summaryDefaults": "Default context / output",
    "settings.customModels.summaryNotEntered": "Not entered",
    "settings.customModels.advancedChangeSummaryDescription": "Review every advanced model definition that will be saved together. Unlisted catalog facts are not copied or replaced.",
    "settings.customModels.summaryAdvancedDefinitions": "Advanced definitions",
    "settings.customModels.summaryCurrentModel": "Current model",
    "settings.customModels.textOnly": "Text only",
    "settings.customModels.advancedUnavailable": "This catalog model has no advanced definition yet.",
    "settings.customModels.advancedCreateHint": "Create a user definition only when protocol parameters or request projection are needed; the catalog itself is not copied.",
    "settings.customModels.advancedCreate": "Create advanced definition",
    "settings.customModels.saveAdvanced": "Save advanced definition",
    "settings.customModels.unsavedAdvanced": "The advanced definition has unsaved changes.",
    "settings.customModels.modelIdRequired": "Enter the real model ID.",
    "settings.customModels.manualDescription": "Add a model manually only when /models is unavailable or omits it. The local common library is checked first.",
    "settings.customModels.templateChoice": "Capability template",
    "settings.customModels.discardTitle": "Discard unsaved changes?",
    "settings.customModels.discardDescription": "The advanced model definition still has unsaved changes. Closing now will discard them.",
    "settings.customModels.discard": "Discard changes",
    "settings.modelCapabilities.title": "Model settings",
    "settings.modelCapabilities.shortLabel": "Capabilities",
    "settings.modelCapabilities.overlayHint": "The current effective values are shown; saving makes this model override the provider settings directly.",
  "settings.modelCapabilities.contextLimit": "Context limit",
  "settings.modelCapabilities.overrideContext": "Override the context limit declared by the common library or provider",
    "settings.modelCapabilities.contextHint": "When no exact declaration exists the common library falls back to 200,000. Override it here for the actual plan.",
    "settings.modelCapabilities.contextInvalid": "The context limit must be an integer greater than zero.",
    "settings.modelCapabilities.inputResources": "Input resource override",
    "settings.modelCapabilities.outputResources": "Output resource override",
    "settings.modelCapabilities.overrideResources": "Override the complete set (an explicit empty set is allowed)",
    "settings.modelCapabilities.restore": "Restore source defaults",
    "settings.modelCapabilities.modelSource": "Model settings",
    "settings.modelCapabilities.inheritedSource": "Inherited provider",
    "settings.contextWindowPolicy.title": "Codex context-window policy",
    "settings.contextWindowPolicy.description": "Calculated from the raw model window: 95% effective window and 90% automatic compaction by default; empty values inherit the stronger layer.",
    "settings.contextWindowPolicy.effectivePercent": "Effective window percent",
    "settings.contextWindowPolicy.autoCompactPercent": "Automatic compaction percent",
    "settings.contextWindowPolicy.autoCompactTokenLimit": "Automatic compaction token limit",
    "settings.contextWindowPolicy.scope": "Compaction usage scope",
    "settings.contextWindowPolicy.scopeInherit": "Inherit stronger layer",
    "settings.contextWindowPolicy.scopeTotal": "Total usage",
    "settings.contextWindowPolicy.scopeBodyAfterPrefix": "Body after stable prefix",
    "settings.contextWindowPolicy.fallbackBuffer": "Fallback buffer tokens",
    "settings.contextWindowPolicy.clear": "Clear this policy layer",
    "settings.models.enabled": "Enabled",
    "settings.models.disabled": "Disabled",
    "settings.models.toggle": "Toggle model availability",
    "settings.models.loadingCatalog": "Loading the live model catalog…",
    "settings.models.provider": "Provider",
    "settings.models.liveModels": "Live models",
    "settings.models.noProviderSelected": "No provider selected",
    "settings.models.catalogSummary": "{provider} · {count} total",
    "settings.models.memoryOnly": "Memory only; updated at startup and after manual synchronization",
    "settings.models.emptyCatalog": "No synchronized models are available. Add an account under Model Providers and synchronize it first.",
    "settings.models.availableAccounts": "{model} · {count} available accounts",
    "settings.models.availableCredentials": "{count} available credentials",
    "settings.models.status": "Status",
    "settings.models.additionalConditions": "{protocol} · Additional conditions",
    "settings.models.collapseProviders": "Collapse to pill",
    "settings.models.expandProviders": "Expand provider categories",
    "settings.models.searchLabel": "Search models",
    "settings.models.searchPlaceholder": "Search model name or ID",
    "settings.models.statusFilter": "Filter models by enabled state",
    "settings.models.filterAll": "All states",
    "settings.models.filterEnabled": "Enabled only",
    "settings.models.filterDisabled": "Disabled only",
    "settings.models.enableAll": "Enable all",
    "settings.models.disableAll": "Disable all",
    "settings.models.filteredSummary": "Showing {visible} of {total} models",
    "settings.models.emptyFilter": "No models match the current filters.",
    "settings.models.inputModalities": "Input",
    "settings.models.outputModalities": "Output",
    "settings.models.modality.text": "Text",
    "settings.models.modality.image": "Image",
    "settings.models.modality.audio": "Audio",
    "settings.models.modality.video": "Video",
    "settings.models.modality.file": "File",
    "settings.models.modality.embedding": "Embedding",
    "settings.models.modality.rerank": "Rerank",
    "settings.inference.title": "Internal inference",
    "settings.inference.subtitle": "Select exact models, protocols, and credentials for VMM and other internal consumers independently from the active conversation model.",
    "settings.inference.vmmReasoningNotice": "To keep memory processing efficient, every VMM LLM request forces reasoning off; the model list shows only models that explicitly support disabled reasoning.",
    "settings.inference.profileVmm": "VMM profile",
    "settings.inference.llm": "Restricted LLM",
    "settings.inference.embedding": "Embeddings",
    "settings.inference.rerank": "Rerank",
    "settings.inference.noRoute": "Not configured",
    "settings.inference.modelRegion": "Model",
    "settings.inference.accountRegion": "Account",
    "settings.inference.selectModel": "Select model",
    "settings.inference.selectAccount": "Select account",
    "settings.inference.selectModelFirst": "Select a model first",
    "settings.inference.accountFilter": "Filter by account, service entry, or protocol",
    "settings.inference.noAccountMatches": "No accounts match this model.",
    "settings.inference.unavailable": "Currently unavailable",
    "settings.inference.unavailableReason": "This route cannot execute: {reason}",
    "settings.inference.testConnection": "Run a real connection test",
    "settings.inference.testEmbeddingSuccess": "Test passed: {count} result(s) · {dimensions} dimensions · {duration} ms",
    "settings.inference.testRerankSuccess": "Test passed: {count} ranked result(s) · {duration} ms",
    "settings.inference.credentialAvailable": "Credential available",
    "settings.inference.catalogGeneration": "Catalog generation {generation}",
    "settings.inference.capabilityRevision": "Capability revision {revision}",
    "settings.inference.scopeInteractive": "Interactive agent",
    "settings.inference.scopeBackground": "Internal background",
    "settings.inference.scopeExternal": "External service",
    "settings.inference.internalToggle": "Toggle internal inference capability",
    "settings.inference.configureDefaults": "Configure internal defaults",
    "settings.inference.overridesTitle": "Internal inference defaults",
    "settings.inference.overridesDescription": "These settings belong to a provider model and operation rather than an account; every exact route is validated on save.",
    "settings.inference.inheritModelDefault": "Use model default",
    "settings.inference.reasoningEffort": "Reasoning effort",
    "settings.inference.embeddingTask": "Embedding task",
    "settings.inference.embeddingDimensions": "Embedding dimensions",
    "settings.inference.embeddingOutputKind": "Embedding output kind",
    "settings.inference.embeddingEncoding": "Embedding encoding",
    "settings.inference.rerankTopN": "Default result count",
    "settings.inference.rerankTruncation": "Truncation policy",
    "settings.inference.rerankReturnContent": "Return candidate content",
    "settings.inference.rerankRestartTitle": "Change reranking settings and restart VMM?",
    "settings.inference.rerankRestartDescription": "Enabling, disabling, or changing the rerank model saves a new configuration and requires a VMM restart to take effect. Continue?",
    "settings.inference.rerankRestartCancel": "Cancel",
    "settings.inference.rerankRestartConfirm": "Save and restart",
    "settings.inference.rerankRestartFailed": "The rerank configuration was saved, but VMM restart failed. Retry from the VMM status page.",
    "settings.inference.maintenanceLocked": "Memory backup and vector rebuild are running. Inference routes are locked to prevent concurrent data-space changes.",
    "settings.inference.purpose.precheckL1": "Fast decision",
    "settings.inference.purpose.precheckL1Description": "PreCheck L1: quickly decides whether this turn needs long-term memory recall.",
    "settings.inference.purpose.precheckL2": "Deep decision",
    "settings.inference.purpose.precheckL2Description": "PreCheck L2: determines which memories to recall for complex input.",
    "settings.inference.purpose.postactionL1": "Fast organization",
    "settings.inference.purpose.postactionL1Description": "PostAction L1: quickly extracts information worth retaining from this turn.",
    "settings.inference.purpose.postactionL2": "Deep organization",
    "settings.inference.purpose.postactionL2Description": "PostAction L2: reviews and merges information into durable memory.",
    "settings.inference.purpose.profileInstruction": "Profile instruction",
    "settings.inference.purpose.profileInstructionDescription": "Applies natural-language corrections to the user or project profile.",
    "settings.inference.purpose.reserve": "Reserved task",
    "settings.inference.purpose.reserveDescription": "Reserved for future VMM internal tasks and not used by the normal memory flow.",
    "settings.providerTools.title": "AI extension capabilities",
    "settings.providerTools.description": "Each capability selects exactly one model and exposes that model's native parameter schema.",
    "settings.providerTools.select": "Select model",
    "settings.providerTools.disabled": "Disabled",
    "settings.providerTools.unavailable": "Selected target unavailable",
    "settings.providerTools.imageGenerate": "Image generation",
    "settings.providerTools.imageEdit": "Image editing tool",
    "settings.providerTools.mediaAnalyze": "Image understanding tool",
    "settings.providerTools.musicGenerate": "Music generation",
    "settings.providerTools.musicCover": "Music cover",
    "settings.providerTools.speechSynthesize": "Speech synthesis",
    "settings.providerTools.speechTranscribe": "Speech transcription",
    "settings.providerTools.videoGenerate": "Video generation",
    "settings.providerTools.videoEdit": "Video editing",
    "settings.providerTools.webSearch": "Web search",
    "settings.providerTools.webExtract": "Web extraction",
    "settings.providerTools.loading": "Loading capability targets…",
    "settings.providerTools.enabled": "Enabled",
    "settings.providerTools.default": "Default target",
    "settings.providerTools.setDefault": "Set as default",
    "settings.providerTools.noTargets": "No current model supports this capability.",
    "settings.providerTools.change": "Change target",
    "settings.providerTools.noSelection": "No target is selected for this capability.",
    "settings.providerTools.configure": "Configure target",
    "settings.providerTools.dialogTitle": "Configure capability target",
    "settings.providerTools.dialogDescription": "Choose a provider on the left and one target for this capability on the right.",
    "settings.providerTools.providers": "Providers",
    "settings.providerTools.targets": "Targets",
    "settings.providerTools.systemDefault": "System defaults",
    "settings.providerTools.systemDefaultHint": "The capability switch, default model, and default account are independent; new conversations copy this snapshot.",
    "settings.providerTools.model": "Model",
    "settings.providerTools.account": "Account",
    "settings.providerTools.selectModel": "Select a model",
    "settings.providerTools.selectAccount": "Select an account",
    "composer.capability.conflict.tooltip": "Capability settings require confirmation",
    "composer.capability.conflict.banner": "A saved model or account is no longer available. Select and save a valid replacement before sending.",
    "composer.capability.conflict.missingSelection": "No model and account are selected for this capability.",
    "composer.capability.conflict.targetUnavailable": "The saved model or account is no longer available.",
    "settings.providerTools.disable": "Disable",
    "settings.providerTools.cancel": "Cancel",
    "settings.providerTools.save": "Save",
    "settings.providerTools.test": "Test",
    "settings.providerTools.testSearchTitle": "Test web search",
    "settings.providerTools.testExtractTitle": "Test web extraction",
    "settings.providerTools.testSearchDescription": "Run one real search with this credential and target without changing the default.",
    "settings.providerTools.testExtractDescription": "Run one real extraction with this credential and target without changing the default.",
    "settings.providerTools.testQuery": "Search query",
    "settings.providerTools.testQueryPlaceholder": "Enter a query to search",
    "settings.providerTools.testMaxResults": "Result count",
    "settings.providerTools.testIncludeDomains": "Include domains",
    "settings.providerTools.testExcludeDomains": "Exclude domains",
    "settings.providerTools.testDomainsPlaceholder": "One domain per line",
    "settings.providerTools.testUrls": "Page URLs",
    "settings.providerTools.testUrlsPlaceholder": "One HTTPS URL per line",
    "settings.providerTools.testExtractQuery": "Relevant content",
    "settings.providerTools.testChunksPerSource": "Chunks per page",
    "settings.providerTools.testDepth": "Extraction depth",
    "settings.providerTools.testDepthBasic": "Basic",
    "settings.providerTools.testDepthAdvanced": "Advanced",
    "settings.providerTools.testFormat": "Content format",
    "settings.providerTools.testFormatMarkdown": "Markdown",
    "settings.providerTools.testFormatText": "Plain text",
    "settings.providerTools.testTimeoutSeconds": "Timeout seconds",
    "settings.providerTools.testIncludeImages": "Include images",
    "settings.providerTools.testIncludeFavicon": "Include favicon",
    "settings.providerTools.testConsumesQuota": "This action calls the real provider API and consumes account quota.",
    "settings.providerTools.testRunning": "Testing…",
    "settings.providerTools.testResults": "Test results",
    "settings.providerTools.testNoResults": "The provider returned no displayable result.",
    "settings.providerTools.testSucceeded": "Succeeded",
    "settings.providerTools.testFailed": "Failed",
    "settings.providerTools.testProviderRequestId": "Provider request ID",
    "toolPresentation.providerText.title": "AI analysis",
    "toolPresentation.providerText.empty": "The provider returned no text result.",
    "toolPresentation.providerSearch.title": "Web search",
    "toolPresentation.providerSearch.results": "results",
    "toolPresentation.providerSearch.empty": "The provider returned no search results.",
    "toolPresentation.providerExtract.title": "Web extraction",
    "toolPresentation.providerExtract.results": "pages",
    "toolPresentation.providerExtract.failures": "failures",
    "toolPresentation.providerExtract.images": "images",
    "toolPresentation.providerExtract.empty": "The provider returned no extraction results.",
    "toolPresentation.providerStructured.embedding": "Embedding result",
    "toolPresentation.providerStructured.rerank": "Rerank result",
    "toolPresentation.providerStructured.composite": "Composite result",
    "toolPresentation.providerStructured.items": "items",
    "settings.modelOverrides.disabled": "Disabled",
    "settings.modelOverrides.cancel": "Cancel",
    "settings.modelOverrides.save": "Save changes",
    "settings.modelOverrides.saving": "Saving…",
    "settings.modelOverrides.validation.safeInteger": "must be a safe integer",
    "settings.modelOverrides.validation.positiveInteger": "must be a positive integer",
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
    "gitCommitDialog.reviewSettings": "Review settings",
    "gitCommitDialog.monitoringEnabled": "Monitoring on",
    "gitCommitDialog.monitoringDisabled": "Monitoring off",
    "gitCommitDialog.monitoringManager": "Git settings",
    "gitCommitDialog.monitoringManagerDescription": "Choose repositories to monitor and adjust their display order. The first enabled repository stays expanded.",
    "gitCommitDialog.moveRepositoryUp": "Move repository up",
    "gitCommitDialog.moveRepositoryDown": "Move repository down",
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
