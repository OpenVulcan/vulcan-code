// LocaleCode describes supported WebUI languages.
// LocaleCode 描述 WebUI 支持的语言。
export type LocaleCode = "zh-CN" | "en-US"

// TranslationKey describes text keys currently localized at the shell level.
// TranslationKey 描述当前在外壳层支持本地化的文本键。
export type TranslationKey =
  | "ui.close"
  | "ui.loading"
  | "gitRepository.list"
  | "gitRepository.primary"
  | "gitRepository.nested"
  | "gitRepository.branchUnknown"
  | "gitRepository.changesUnknown"
  | "gitRepository.changedFiles"
  | "gitRepository.refreshing"
  | "gitRepository.readFailed"
  | "gitRepository.busy"
  | "gitRepository.loading"
  | "gitRepository.unavailable"
  | "gitRepository.retryReview"
  | "projectOverview.conversations"
  | "projectOverview.pinned"
  | "projectOverview.issues"
  | "projectOverview.memoryLoading"
  | "projectOverview.memoryUnknown"
  | "projectOverview.native"
  | "projectOverview.external"
  | "projectOverview.enabled"
  | "projectOverview.disabled"
  | "projectOverview.override"
  | "projectOverview.inherited"
  | "projectOverview.unsaved"
  | "projectOverview.missingOwner"
  | "projectOverview.ownerUnknown"
  | "projectOverview.nativeBinding"
  | "projectOverview.unbound"
  | "projectOverview.bindingUnknown"
  | "projectOverview.externalBinding"
  | "projectOverview.externalUnbound"
  | "projectOverview.precheck"
  | "projectOverview.writeback"
  | "projectOverview.on"
  | "projectOverview.off"
  | "projectOverview.session"
  | "projectOverview.scopeReady"
  | "projectOverview.scopeNotReady"
  | "projectOverview.precheckReady"
  | "projectOverview.precheckNotReady"
  | "projectOverview.postactionReady"
  | "projectOverview.postactionNotReady"
  | "projectOverview.runtimeUnknown"
  | "projectOverview.memorySettings"
  | "projectOverview.codeSettings"
  | "projectOverview.knowledgeSettings"
  | "projectOverview.skillSettings"
  | "projectOverview.indexDisabled"
  | "projectOverview.queryReady"
  | "projectOverview.queryUnavailable"
  | "projectOverview.indexUnknown"
  | "projectOverview.watching"
  | "projectOverview.notWatching"
  | "projectOverview.pending"
  | "projectOverview.files"
  | "projectOverview.routesUnavailable"
  | "projectOverview.knowledgeCount"
  | "projectOverview.knowledgeFailed"
  | "projectOverview.knowledgeLoading"
  | "projectOverview.skillsLoading"
  | "projectOverview.skillsUnknown"
  | "projectOverview.skillsCount"
  | "projectOverview.skillsInherited"
  | "projectOverview.maintenance"
  | "projectOverview.attachments"
  | "projectOverview.details"
  | "projectOverview.path"
  | "projectOverview.copyPath"
  | "projectOverview.copy"
  | "projectOverview.copied"
  | "projectOverview.copyFailed"
  | "projectOverview.attachmentPath"
  | "projectOverview.id"
  | "settings.modelExperience.retryLoad"
  | "settings.modelExperience.context"
  | "settings.modelExperience.invalidValue"
  | "settings.modelExperience.extensionSelectionHint"
  | "settings.modelExperience.retainedEmbeddingInvalid"
  | "settings.licenses.title"
  | "settings.licenses.types"
  | "settings.licenses.dependencies"
  | "settings.licenses.scope"
  | "settings.licenses.search"
  | "settings.licenses.reload"
  | "settings.licenses.loadError"
  | "settings.licenses.loading"
  | "settings.licenses.emptySearch"
  | "settings.licenses.unknown"
  | "settings.licenses.variants"
  | "settings.licenses.dependency"
  | "settings.licenses.version"
  | "settings.licenses.source"
  | "settings.licenses.body"
  | "settings.licenses.view"
  | "settings.licenses.previous"
  | "settings.licenses.next"
  | "settings.licenses.variant"
  | "settings.licenses.variantHint"
  | "settings.licenses.retry"
  | "settings.licenses.loadingBody"
  | "settings.modelExperience.conversation"
  | "settings.modelExperience.nodes"
  | "settings.modelExperience.retry"
  | "settings.modelExperience.media"
  | "settings.modelExperience.usage"
  | "settings.modelExperience.scopeHint"
  | "settings.modelExperience.inheritWindow"
  | "settings.modelExperience.inheritCompact"
  | "settings.modelExperience.inheritToken"
  | "settings.modelExperience.inheritScope"
  | "settings.modelExperience.inheritBuffer"
  | "settings.modelExperience.savingSystem"
  | "settings.modelExperience.dirtySystem"
  | "settings.modelExperience.savedSystem"
  | "settings.modelExperience.savingNodes"
  | "settings.modelExperience.dirtyNodes"
  | "settings.modelExperience.savedNodes"
  | "settings.modelExperience.nodeIds"
  | "settings.modelExperience.statistics"
  | "settings.modelExperience.searchCustom"
  | "settings.modelExperience.searchProvider"
  | "settings.modelExperience.searchPlaceholder"
  | "settings.modelExperience.noProviders"
  | "settings.modelExperience.providerDetails"
  | "settings.modelExperience.usageFilter"
  | "settings.modelExperience.allUsages"
  | "settings.modelExperience.conversationEnabled"
  | "settings.modelExperience.internalEnabled"
  | "settings.modelExperience.multimodalEnabled"
  | "settings.modelExperience.capabilityFilter"
  | "settings.modelExperience.allCapabilities"
  | "settings.modelExperience.routes"
  | "settings.modelExperience.modelColumn"
  | "settings.modelExperience.understandingUse"
  | "settings.modelExperience.accessButton"
  | "settings.modelExperience.candidateExplanation"
  | "settings.modelExperience.routeSettings"
  | "settings.modelExperience.bindingSource"
  | "settings.modelExperience.routeAccounts"
  | "settings.modelExperience.available"
  | "settings.modelExperience.accountNotAdmitted"
  | "settings.modelExperience.internalPurposes"
  | "settings.modelExperience.candidateHint"
  | "settings.modelExperience.noConversation"
  | "settings.modelExperience.unavailableCandidates"
  | "settings.modelExperience.disableCandidate"
  | "settings.modelExperience.curationModel"
  | "settings.modelExperience.embeddingModel"
  | "settings.modelExperience.modelSearch"
  | "settings.modelExperience.curationNoMatch"
  | "settings.modelExperience.modelNoMatch"
  | "knowledge.experience.targetChanged"
  | "knowledge.experience.createTitle"
  | "knowledge.experience.retrievalMode"
  | "knowledge.experience.rawMode"
  | "knowledge.experience.displayMode"
  | "knowledge.experience.hybridMode"
  | "knowledge.experience.reasoning"
  | "knowledge.experience.reasoningPlaceholder"
  | "knowledge.experience.reasoningHint"
  | "knowledge.experience.dimensions"
  | "knowledge.experience.dimensionsPlaceholder"
  | "knowledge.experience.dimensionsInput"
  | "knowledge.experience.dimensionsInvalid"
  | "knowledge.experience.batch"
  | "knowledge.experience.batchFallback"
  | "knowledge.experience.compatibility"
  | "knowledge.experience.compatibilityPlaceholder"
  | "knowledge.experience.compatibilityHint"
  | "knowledge.experience.compatibilityActive"
  | "knowledge.experience.browseCorpus"
  | "knowledge.experience.rawLibrary"
  | "knowledge.experience.displayLibrary"
  | "knowledge.experience.searchLayer"
  | "knowledge.experience.rawSearch"
  | "knowledge.experience.displaySearch"
  | "knowledge.experience.hybridSearch"
  | "knowledge.experience.viewProgress"
  | "knowledge.experience.resume"
  | "knowledge.experience.displayReady"
  | "knowledge.experience.regenerate"
  | "knowledge.experience.buildDisplay"
  | "knowledge.experience.buildVectors"
  | "knowledge.experience.saveSettings"
  | "knowledge.experience.configure"
  | "knowledge.experience.settingsTitle"
  | "knowledge.experience.displayOperation"
  | "knowledge.experience.vectorOperation"
  | "knowledge.experience.background"
  | "knowledge.experience.curating"
  | "knowledge.experience.curatingNow"
  | "knowledge.experience.embeddingNow"
  | "knowledge.experience.curatingProgress"
  | "knowledge.experience.embeddingProgress"
  | "knowledge.experience.cancelling"
  | "knowledge.experience.failed"
  | "knowledge.experience.cancelled"
  | "knowledge.experience.completed"
  | "knowledge.experience.running"
  | "knowledge.experience.waiting"
  | "knowledge.experience.processing"
  | "knowledge.experience.items"
  | "knowledge.experience.llmRequests"
  | "knowledge.experience.embeddingRequests"
  | "knowledge.experience.retries"
  | "knowledge.experience.generated"
  | "knowledge.experience.inputTokens"
  | "knowledge.experience.outputTokens"
  | "knowledge.experience.cumulative"
  | "knowledge.experience.cacheTokens"
  | "knowledge.experience.currentRequest"
  | "knowledge.experience.noRequest"
  | "knowledge.experience.requestTokens"
  | "knowledge.experience.preparing"
  | "knowledge.experience.waitingStream"
  | "knowledge.experience.recentRequests"
  | "knowledge.experience.noHistory"
  | "knowledge.experience.currentFile"
  | "knowledge.experience.unknown"
  | "settings.modelExperience.embeddingNoMatch"
  | "settings.modelExperience.curationUnavailable"
  | "settings.modelExperience.embeddingUnavailable"
  | "settings.modelExperience.chooseCuration"
  | "settings.modelExperience.chooseEmbedding"
  | "settings.modelExperience.noCuration"
  | "settings.modelExperience.noEmbedding"
  | "memory.workspace.read"
  | "memory.workspace.purpose"
  | "memory.workspace.actions"
  | "memory.workspace.manage"
  | "memory.workspace.settings"
  | "memory.workspace.service"
  | "memory.workspace.browse"
  | "memory.workspace.search"
  | "memory.workspace.searchScope"
  | "memory.workspace.filters"
  | "memory.workspace.all"
  | "memory.workspace.bound"
  | "memory.workspace.unbound"
  | "memory.workspace.overrides"
  | "memory.workspace.noMatches"
  | "memory.workspace.selectedCount"
  | "memory.workspace.clearFilters"
  | "memory.workspace.dateHint"
  | "memory.workspace.updatesAvailable"
  | "memory.workspace.missingOutputs"
  | "memory.workspace.missingSource"
  | "memory.workspace.disabled"
  | "memory.workspace.not_ready"
  | "memory.workspace.ready"
  | "memory.workspace.selectSession"
  | "memory.workspace.editScope"
  | "memory.workspace.basic"
  | "memory.workspace.globalPolicy"
  | "memory.workspace.recallPolicy"
  | "memory.workspace.textPolicy"
  | "memory.workspace.profilePolicy"
  | "memory.workspace.rankingPolicy"
  | "memory.workspace.retentionPolicy"
  | "memory.advanced.catalogLoading"
  | "memory.advanced.catalogFailed"
  | "memory.advanced.catalogRetry"
  | "memory.workspace.scopeHint"
  | "memory.workspace.recordInfo"
  | "memory.workspace.projectRules"
  | "memory.workspace.effectiveFiles"
  | "memory.workspace.otherFiles"
  | "memory.workspace.index"
  | "memory.workspace.cleanup"
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
  | "area.contextEngine"
  | "area.knowledge"
  | "area.settings"
  | "area.chat.description"
  | "area.skills.description"
  | "area.agentSkills.description"
  | "area.agents.description"
  | "area.memory.description"
  | "area.contextEngine.description"
  | "area.knowledge.description"
  | "knowledge.name"
  | "knowledge.name.placeholder"
  | "knowledge.create"
  | "knowledge.filter.aria"
  | "knowledge.filter.placeholder"
  | "knowledge.refresh"
  | "knowledge.emptyLibraries"
  | "knowledge.librariesLoading"
  | "knowledge.noMatchingLibraries"
  | "knowledge.readFailed"
  | "knowledge.libraryCounts"
  | "knowledge.indexStatus.empty"
  | "knowledge.indexStatus.ready"
  | "knowledge.indexStatus.dirty"
  | "knowledge.indexStatus.rebuilding"
  | "knowledge.indexStatus.failed"
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
  | "knowledge.composer.inherit"
  | "knowledge.composer.loading"
  | "knowledge.composer.readFailed"
  | "knowledge.composer.saveFailed"
  | "knowledge.composer.reloadFailed"
  | "knowledge.composer.retry"
  | "knowledge.composer.details"
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
  | "appChrome.themeSaveFailed"
  | "appChrome.language.zh"
  | "appChrome.scrollFeaturesLeft"
  | "appChrome.scrollFeaturesRight"
  | "appChrome.windowActionFailed"
  | "appChrome.window.minimize"
  | "appChrome.window.restore"
  | "appChrome.window.maximize"
  | "appChrome.window.close"
  | "app.shutdown.persisting"
  | "app.shutdown.persistFailed.title"
  | "app.shutdown.persistFailed.description"
  | "app.shutdown.persistFailed.forceConfirm"
  | "app.shutdown.persistFailed.cancel"
  | "app.shutdown.persistFailed.forceClose"
  | "app.shutdown.persistFailed.forceClosing"
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
  | "projectMenu.openTerminal"
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
  | "conversation.submission.uploadFailed"
  | "conversation.submission.uploadFailedAttachmentKept"
  | "conversation.submission.uploadProgress"
  | "conversation.submission.sizeUnknown"
  | "conversation.submission.cancelFailed"
  | "conversation.submission.cancelling"
  | "conversation.loading.requesting"
  | "conversation.loading.locating"
  | "conversation.loading.index"
  | "conversation.loading.reading"
  | "conversation.loading.subagents"
  | "conversation.loading.projecting"
  | "conversation.loading.done"
  | "conversation.loading.failed"
  | "conversation.loading.retry"
  | "conversation.unlinkedProject"
  | "conversation.history.loadMore"
  | "conversation.history.loading"
  | "conversation.history.invalidResponse"
  | "conversation.history.invalidCursor"
  | "conversation.history.emptyPageWithMore"
  | "conversation.history.loadFailed"
  | "conversation.action.copyFinal"
  | "conversation.action.copiedFinal"
  | "conversation.action.fork"
  | "conversation.action.forkTitleSuffix"
  | "conversation.action.forkFailed"
  | "conversation.action.forkSucceeded"
  | "conversation.action.forkInProgressTitle"
  | "conversation.action.forkInProgressMessage"
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
  | "conversation.systemError.pendingTitle"
  | "conversation.systemError.pendingStatus"
  | "conversation.systemError.pendingNotice"
  | "conversation.systemError.resultConfirmation"
  | "conversation.systemError.usageConfirmation"
  | "conversation.systemError.unconfirmed"
  | "conversation.systemError.requestIdentifier"
  | "conversation.systemError.requestIdentifierSource"
  | "conversation.systemError.upstreamErrorPayload"
  | "conversation.systemError.identifierMissing"
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
  | "conversation.providerRetry.retrying"
  | "conversation.providerRetry.succeeded"
  | "conversation.providerRetry.failed"
  | "conversation.modelSwitch.title"
  | "conversation.modelSwitch.changed"
  | "conversation.goalNotice.saved"
  | "conversation.goalNotice.resumed"
  | "conversation.goalNotice.paused"
  | "conversation.goalNotice.blocked"
  | "conversation.goalNotice.completed"
  | "conversation.goalNotice.cleared"
  | "conversation.goalNotice.budgetExhausted"
  | "conversation.goalNotice.usageLimited"
  | "conversation.goalNotice.usageUnknown"
  | "conversation.goalNotice.updated"
  | "composer.draft.retry"
  | "conversation.modelSwitch.help"
  | "conversation.modelSwitch.warning"
  | "conversation.reasoningSwitch.title"
  | "conversation.reasoningSwitch.nextProviderRequest"
  | "conversation.continue"
  | "conversation.continuing"
  | "conversation.continuation.claimFailed"
  | "conversation.continuation.blocked"
  | "conversation.continuation.blocked.default"
  | "conversation.continuation.blocked.activeApi"
  | "conversation.continuation.blocked.missingApiStarted"
  | "conversation.continuation.blocked.duplicateApiTerminal"
  | "conversation.continuation.blocked.missingApiTerminal"
  | "conversation.continuation.blocked.apiTerminalOrder"
  | "conversation.continuation.blocked.terminalBoundarySuperseded"
  | "conversation.continuation.blocked.terminalPolicyForbidden"
  | "conversation.continuation.blocked.routeChanged"
  | "conversation.continuation.blocked.sessionBusy"
  | "conversation.continuation.blocked.requestInvalid"
  | "conversation.continuation.blocked.conversationUnavailable"
  | "conversation.continuation.blocked.runtimeUnavailable"
  | "conversation.continuation.blocked.unknown"
  | "conversation.continuation.timeout"
  | "conversation.continuation.stateSyncFailed"
  | "conversation.continuation.ackMismatch"
  | "conversation.metadata.type"
  | "conversation.metadata.status"
  | "conversation.metadata.tool"
  | "conversation.metadata.identifier"
  | "conversation.metadata.error"
  | "conversation.metadata.result"
  | "conversation.metadata.returned"
  | "conversation.toolRequest.receiving"
  | "conversation.toolRequest.argumentsAssembled"
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
  | "conversation.process.working"
  | "conversation.process.worked"
  | "conversation.process.stoppedAfter"
  | "conversation.process.stopped"
  | "conversation.process.duration"
  | "conversation.process.providerUsageBadge"
  | "conversation.process.providerUsageSession"
  | "conversation.process.providerUsageUnknown"
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
  | "composer.capability.loading"
  | "composer.capability.loadFailed"
  | "composer.capability.retry"
  | "composer.capability.unavailableEnabled"
  | "composer.capability.disableUnavailable"
  | "composer.capability.removeUnavailable"
  | "composer.capability.unavailableDetails"
  | "composer.capability.operationFailed"
  | "composer.capability.errorDetails"
  | "composer.capability.rebuild"
  | "composer.capability.rebuilding"
  | "composer.capability.rebuildHint"
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
  | "composer.contextUsage.cacheHitRate"
  | "composer.contextUsage.noData"
  | "composer.contextUsage.usedPercent"
  | "composer.contextUsage.totalRequests"
  | "composer.contextUsage.averageFirstToken"
  | "composer.contextUsage.averageSpeed"
  | "composer.contextUsage.noTiming"
  | "composer.contextUsage.totalTokens"
  | "composer.capability.mediaBackendMode"
  | "composer.capability.mediaFixedRoute"
  | "composer.capability.mediaBackendNote"
  | "composer.capability.mediaBackendMissing"
  | "composer.attachment.previewUnavailable"
  | "composer.attachment.stagingPending"
  | "composer.capability.extensionHint"
  | "composer.capability.extensionProfileMissing"
  | "composer.capability.extensionConstraints"
  | "composer.capability.extensionLimitations"
  | "composer.capability.mode.provider"
  | "composer.capability.mode.model"
  | "composer.capability.reasoning"
  | "composer.capability.inheritGlobalReasoning"
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
  | "composer.contextWindow.title"
  | "composer.contextWindow.default"
  | "composer.contextWindow.invalid"
  | "composer.contextWindow.locked"
  | "composer.contextWindow.defaultRestored"
  | "composer.contextWindow.previewUnavailable"
  | "composer.contextWindow.saveFailed"
  | "composer.modelPicker.filter"
  | "composer.modelPicker.noMatches"
  | "composer.attachment.add"
  | "composer.attachment.extension"
  | "composer.attachment.menu"
  | "composer.attachment.empty"
  | "composer.attachment.remove"
  | "composer.attachment.processing"
  | "composer.attachment.preparationBlocked"
  | "composer.attachment.metadataOnly"
  | "composer.attachment.details"
  | "composer.attachment.failed"
  | "composer.attachment.retry"
  | "composer.attachment.unsupported"
  | "composer.attachment.maxLimit"
  | "composer.attachment.partialMaxLimit"
  | "composer.attachment.kind.image"
  | "composer.attachment.kind.audio"
  | "composer.attachment.kind.video"
  | "composer.attachment.kind.pdf"
  | "composer.attachment.kind.file"
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
  | "composer.goal.duration"
  | "composer.goal.usage"
  | "composer.goal.requests"
  | "composer.goal.tools"
  | "composer.goal.cacheHit"
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
  | "composer.error.draftLoadFailed"
  | "composer.error.draftPersistFailed"
  | "composer.error.draftChangedBeforeSubmission"
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
  | "composer.accountCard.commandCodeMonthlyCredits"
  | "composer.accountCard.commandCodePurchasedCredits"
  | "composer.accountCard.commandCodeFreeCredits"
  | "composer.accountCard.commandCodeFiveHourRequests"
  | "composer.accountCard.commandCodeWeeklyRequests"
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
  | "conversationActions.previewCannotContinue"
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
  | "projectSettings.knowledgeBasesSavedRefreshFailed"
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
  | "projectSettings.codeContext.title"
  | "projectSettings.codeContext.enabled"
  | "projectSettings.codeContext.disabled"
  | "projectSettings.codeContext.generating"
  | "projectSettings.vmmDescription"
  | "projectSettings.vmmNativeDescription"
  | "projectSettings.vmmLoading"
  | "projectSettings.enableProjectOverride"
  | "projectSettings.overrideEnabledHint"
  | "projectSettings.overrideDisabledHint"
  | "projectSettings.vmmTransportHint"
  | "projectSettings.vmmNativeHostHint"
  | "projectSettings.vmmNativeEnabled"
  | "projectSettings.vmmAdvanced"
  | "projectSettings.saveVmm"
  | "projectSettings.vmmNativeBinding"
  | "projectSettings.vmmNativeOwner"
  | "projectSettings.vmmNativeLinked"
  | "projectSettings.vmmNativeSaveBinding"
  | "projectSettings.vmmCreateBinding"
  | "projectSettings.vmmCreateAndBind"
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
  | "workspaceAudit.connecting"
  | "workspaceAudit.terminalFailure"
  | "workspaceAudit.exited"
  | "workspaceAudit.exitedWithCode"
  | "workspaceAudit.stopped"
  | "workspaceAudit.failed"
  | "workspaceAudit.copyTruncated"
  | "workspaceAudit.retry"
  | "workspaceAudit.viewportLoading"
  | "workspaceAudit.aiReadOnly"
  | "workspaceAudit.restoreAi"
  | "workspaceAudit.maximizeAi"
  | "workspaceAudit.minimizeAi"
  | "workspaceAudit.startupSteps"
  | "workspaceAudit.terminalLoading"
  | "workspaceAudit.emptyName"
  | "workspaceAudit.terminal"
  | "workspaceAudit.expandList"
  | "workspaceAudit.collapseList"
  | "workspaceAudit.restore"
  | "workspaceAudit.maximize"
  | "workspaceAudit.minimize"
  | "workspaceAudit.numberedList"
  | "workspaceAudit.switchTerminal"
  | "workspaceAudit.chooseShell"
  | "workspaceAudit.create"
  | "workspaceAudit.list"
  | "workspaceAudit.name"
  | "workspaceAudit.saveName"
  | "workspaceAudit.editName"
  | "workspaceAudit.stop"
  | "workspaceAudit.close"
  | "workspaceAudit.stopTitle"
  | "workspaceAudit.closeTitle"
  | "workspaceAudit.empty"
  | "workspaceAudit.closeConfirm"
  | "workspaceAudit.stopConfirm"
  | "workspaceAudit.closeDescription"
  | "workspaceAudit.stopDescription"
  | "workspaceAudit.cancel"
  | "workspaceAudit.processing"
  | "workspaceAudit.confirm"
  | "workspaceAudit.openDirectory"
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
  | "conversation.subagent.state.closed"
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
  | "conversation.subagent.detail.usage"
  | "conversation.subagent.detail.result"
  | "conversation.subagent.detail.noResult"
  | "conversation.subagent.detail.target"
  | "conversation.subagent.detail.queued"
  | "conversation.subagent.detail.steering"
  | "conversation.subagent.queue.steering"
  | "conversation.subagent.queue.followup"
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
| "toolPresentation.output.loadFailed"
| "toolPresentation.output.retryLoad"
| "toolPresentation.output.errorDetails"
  | "toolPresentation.output.waiting"
  | "toolPresentation.output.empty"
  | "toolPresentation.payload.input"
  | "toolPresentation.payload.output"
  | "toolPresentation.modelExtension.parameters"
  | "toolPresentation.raw.open"
  | "toolPresentation.raw.title"
  | "toolPresentation.raw.legacyPartial"
  | "toolPresentation.execution.running"
  | "toolPresentation.exitCode"
  | "toolPresentation.command.terminalTitle"
  | "toolPresentation.command.shellTitle"
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
  | "toolPresentation.native.todolist"
  | "toolPresentation.native.getGoal"
  | "toolPresentation.native.createGoal"
  | "toolPresentation.native.updateGoal"
  | "toolPresentation.native.knowledgeSearch"
  | "toolPresentation.native.knowledgeBrowse"
  | "toolPresentation.native.knowledgeRead"
  | "toolPresentation.native.skill"
  | "toolPresentation.native.memorySearch"
  | "toolPresentation.native.turnDetails"
  | "toolPresentation.native.memoryWrite"
  | "toolPresentation.native.memoryDelete"
  | "toolPresentation.native.runtimeConfig"
  | "toolPresentation.file.count"
  | "toolPresentation.file.multiChange"
  | "toolPresentation.file.empty"
  | "toolPresentation.file.mixedLineEndings"
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
  | "toolPresentation.file.edited"
  | "toolPresentation.file.notEdited"
  | "toolPresentation.file.executionFailed"
  | "toolPresentation.file.executionStopped"
  | "toolPresentation.file.executionCompleted"
  | "toolPresentation.markdownOutput.running"
  | "toolPresentation.markdownOutput.failed"
  | "toolPresentation.markdownOutput.stopped"
  | "toolPresentation.markdownOutput.completed"
  | "toolPresentation.fileList.title"
  | "toolPresentation.fileRead.title"
  | "toolPresentation.fileRead.rangeFrom"
  | "toolPresentation.fileRead.range"
  | "toolPresentation.fileSearch.title"
  | "toolPresentation.fileStat.title"
  | "toolPresentation.codekit.astTree.title"
  | "toolPresentation.codekit.astDetail.title"
  | "toolPresentation.codekit.rg.title"
  | "toolPresentation.codekit.nodeSource.title"
  | "toolPresentation.codekit.markdownMenu.title"
  | "markdown.wrap.enable"
  | "markdown.wrap.disable"
  | "markdown.copyCode"
  | "conversation.control.copyFailed"
  | "conversation.control.actionFailed"
  | "conversation.control.detailFailed"
  | "conversation.control.retry"
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
  | "memory.overview.title"
  | "memory.overview.description"
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
  | "vmm.experience.defaultMemoryMode"
  | "vmm.experience.nativeBindingFixed"
  | "vmm.experience.nativeBindingPerProject"
  | "vmm.experience.refresh"
  | "vmm.experience.start"
  | "vmm.experience.restart"
  | "vmm.experience.stop"
  | "vmm.experience.reconfigure"
  | "vmm.experience.openAdvanced"
  | "memory.identity.title"
  | "memory.identity.description"
  | "memory.identity.nativeTitle"
  | "memory.identity.nativeDescription"
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
  | "memory.content.retrieval"
  | "memory.content.finalScore"
  | "memory.content.creationSource"
  | "memory.content.totalEvidence"
  | "memory.content.matchedEvidence"
  | "memory.content.contextAdjustment"
  | "memory.content.contextNotObserved"
  | "memory.content.description"
  | "memory.content.nativeDescription"
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
  | "memory.nativeRecords.title"
  | "memory.sessionList.title"
  | "memory.sessionList.description"
  | "memory.sessionList.back"
  | "memory.sessionList.projectSources"
  | "memory.sessionList.search"
  | "memory.sessionList.status"
  | "memory.sessionList.extraction"
  | "memory.sessionList.allStates"
  | "memory.sessionList.allExtraction"
  | "memory.sessionList.from"
  | "memory.sessionList.before"
  | "memory.sessionList.sort"
  | "memory.sessionList.newest"
  | "memory.sessionList.oldest"
  | "memory.sessionList.titleSort"
  | "memory.sessionList.apply"
  | "memory.sessionList.loading"
  | "memory.sessionList.failed"
  | "memory.sessionList.detailFailed"
  | "memory.sessionList.invalidDates"
  | "memory.sessionList.selectPage"
  | "memory.sessionList.selected"
  | "memory.sessionList.mixed"
  | "memory.sessionList.select"
  | "memory.sessionList.empty"
  | "memory.sessionList.total"
  | "memory.sessionList.previous"
  | "memory.sessionList.next"
  | "memory.sessionArchive.active"
  | "memory.sessionArchive.archived"
  | "memory.sessionArchive.recycled"
  | "memory.sessionArchive.forgotten"
  | "memory.sessionArchive.unregistered"
  | "memory.sessionArchive.archive"
  | "memory.sessionArchive.unarchive"
  | "memory.sessionArchive.description"
  | "memory.sessionArchive.confirm"
  | "memory.removal.sessions"
  | "memory.removal.turns"
  | "memory.removal.selectSession"
  | "memory.removal.selectTurn"
  | "memory.removal.title"
  | "memory.removal.description"
  | "memory.removal.sessionEffect"
  | "memory.removal.turnEffect"
  | "memory.removal.vectors"
  | "memory.removal.hotTurns"
  | "memory.removal.hotWindow"
  | "memory.removal.context"
  | "memory.removal.projectProfiles"
  | "memory.removal.userProfiles"
  | "memory.removal.retention"
  | "memory.removal.days"
  | "memory.removal.expirySuffix"
  | "memory.rules.invalidNumbers"
  | "memory.rules.features"
  | "memory.rules.retentionProfiles"
  | "memory.advanced.invalidNumbers"
  | "memory.projectProfile.durationHint"
  | "memory.removal.sources"
  | "memory.removal.confirm"
  | "memory.removal.failed"
  | "memory.sessionRestart.action"
  | "memory.sessionRestart.description"
  | "memory.sessionRestart.confirm"
  | "memory.sessionRestart.failed"
  | "memory.sessionArchive.failed"
  | "memory.nativeRecords.description"
  | "memory.nativeRecords.project"
  | "memory.nativeRecords.session"
  | "memory.nativeRecords.selectProject"
  | "memory.nativeRecords.selectSession"
  | "memory.nativeRecords.sessionOverview"
  | "memory.nativeRecords.refresh"
  | "memory.nativeRecords.noProjects"
  | "memory.nativeRecords.noSessions"
  | "memory.nativeRecords.noProjectRecords"
  | "memory.nativeRecords.sessions"
  | "memory.nativeRecords.turns"
  | "memory.nativeRecords.memories"
  | "memory.nativeRecords.profileNodes"
  | "memory.nativeRecords.completed"
  | "memory.nativeRecords.pending"
  | "memory.nativeRecords.processedUnknown"
  | "memory.nativeRecords.bypassed"
  | "memory.nativeRecords.corrupted"
  | "memory.nativeRecords.lastActivity"
  | "memory.nativeRecords.projectSummary"
  | "memory.nativeRecords.turnRecords"
  | "memory.nativeRecords.projectTurnRecords"
  | "memory.nativeRecords.sessionMemories"
  | "memory.nativeRecords.projectMemories"
  | "memory.nativeRecords.projectOnly"
  | "memory.nativeRecords.noTurns"
  | "memory.nativeRecords.noMemories"
  | "memory.nativeRecords.generated"
  | "memory.nativeRecords.generatedMemories"
  | "memory.nativeRecords.noGeneratedMemories"
  | "memory.nativeRecords.generatedProfiles"
  | "memory.nativeRecords.noGeneratedProfiles"
  | "memory.nativeRecords.noProfileNodes"
  | "memory.nativeRecords.noUserPreview"
  | "memory.nativeRecords.retryCount"
  | "memory.nativeRecords.sourceTurn"
  | "memory.nativeRecords.priority"
  | "memory.nativeRecords.tags"
  | "memory.nativeRecords.statusReason"
  | "memory.nativeRecords.expiresAt"
  | "memory.nativeRecords.turn"
  | "memory.nativeRecords.timeline"
  | "memory.nativeRecords.previousTurns"
  | "memory.nativeRecords.nextTurns"
  | "memory.nativeRecords.analysisDetails"
  | "memory.nativeRecords.lastError"
  | "memory.nativeRecords.statusPending"
  | "memory.nativeRecords.statusCompleted"
  | "memory.nativeRecords.statusProcessedUnknown"
  | "memory.nativeRecords.statusBypassed"
  | "memory.nativeRecords.statusCorrupted"
  | "memory.nativeRecords.levelSession"
  | "memory.nativeRecords.levelPhase"
  | "memory.nativeRecords.levelStable"
  | "memory.nativeRecords.levelPersistent"
  | "memory.nativeRecords.stateActive"
  | "memory.nativeRecords.stateSuperseded"
  | "memory.nativeRecords.stateDeleted"
  | "memory.nativeRecords.stateExpired"
  | "memory.nativeRecords.stateRecycled"
  | "memory.nativeRecords.sourceTurnExtracted"
  | "memory.nativeRecords.sourceDirect"
  | "memory.nativeRecords.sourceImport"
  | "memory.nativeRecords.sourceSystemSeed"
  | "memory.nativeRecords.sourceLegacy"
  | "memory.nativeRecords.categoryGeneral"
  | "memory.nativeRecords.categoryArchitecture"
  | "memory.nativeRecords.categoryTechSpec"
  | "memory.nativeRecords.categoryBusiness"
  | "memory.nativeRecords.categoryRequirement"
  | "memory.nativeRecords.categoryProject"
  | "memory.nativeRecords.categoryBugDebt"
  | "memory.nativeRecords.categorySecurity"
  | "memory.nativeRecords.roleUser"
  | "memory.nativeRecords.roleAssistant"
  | "memory.nativeRecords.roleSystem"
  | "memory.nativeRecords.roleTool"
  | "memory.data.title"
  | "memory.data.description"
  | "memory.data.managementReady"
  | "memory.data.managementUnavailable"
  | "memory.data.extractionRuntimeTitle"
  | "memory.data.extractionRuntimeWaiting"
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
  | "memory.profile.nativeDescription"
  | "memory.profile.target"
  | "memory.profile.user"
  | "memory.profile.project"
  | "memory.profile.nativeUserName"
  | "memory.profile.nativeUserDescription"
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
  | "memory.profile.invalidBundleMode"
  | "memory.profile.previewDescription"
  | "memory.profile.nativePreviewDescription"
  | "memory.profile.priority"
  | "memory.profile.level"
  | "memory.profile.stateSummary"
  | "memory.profile.stateSummaryDescription"
  | "memory.profile.stateActive"
  | "memory.profile.statePending"
  | "memory.profile.stateScheduled"
  | "memory.profile.stateInactive"
  | "memory.profile.stateExpired"
  | "memory.profile.notInjected"
  | "memory.profile.source"
  | "memory.profile.sourceTurns"
  | "memory.profile.statusReason"
  | "memory.profile.updatedAt"
  | "memory.profile.storedState"
  | "memory.profile.sourceTurnExtract"
  | "memory.profile.sourceManualInstruction"
  | "memory.profile.sourceSystemSeed"
  | "memory.profile.sourceRetained"
  | "memory.advanced.title"
  | "memory.advanced.description"
  | "memory.advanced.deployment"
  | "memory.advanced.native"
  | "memory.advanced.nativeDescription"
  | "memory.advanced.external"
  | "memory.advanced.externalDescription"
  | "memory.advanced.connection"
  | "memory.advanced.endpoint"
  | "memory.advanced.apiKey"
  | "memory.advanced.managementEndpoint"
  | "memory.advanced.managementToken"
  | "memory.advanced.testManagement"
  | "memory.advanced.managementTimeout"
  | "memory.advanced.defaultBinding"
  | "memory.advanced.nativeFixedProject"
  | "memory.advanced.externalPrefix"
  | "memory.advanced.nativeStorage"
  | "memory.advanced.nativeStorageDescription"
  | "memory.advanced.runtime"
  | "memory.advanced.autoStart"
  | "memory.advanced.manualStart"
  | "memory.advanced.transport"
  | "memory.advanced.postactionReliability"
  | "memory.advanced.postactionTimeout"
  | "memory.advanced.failurePassThreshold"
  | "memory.advanced.maxConnectionsPerHost"
  | "memory.advanced.externalReliability"
  | "memory.advanced.externalReliabilityDescription"
  | "memory.advanced.diagnostics"
  | "memory.advanced.diagnosticRuntime"
  | "memory.advanced.diagnosticBackend"
  | "memory.advanced.diagnosticGrpc"
  | "memory.advanced.diagnosticController"
  | "memory.advanced.diagnosticData"
  | "memory.advanced.diagnosticLog"
  | "memory.advanced.data"
  | "memory.advanced.save"
  | "memory.advanced.backupRebuild"
  | "memory.resources.title"
  | "memory.resources.description"
  | "memory.resources.invalidResponse"
  | "memory.policy.title"
  | "memory.textPolicy.title"
  | "memory.hostPolicy.title"
  | "memory.hostPolicy.description"
  | "memory.hostPolicy.inherited"
  | "memory.hostPolicy.overridden"
  | "memory.hostPolicy.inherit"
  | "memory.hostPolicy.saved"
  | "memory.hostPolicy.connection"
  | "memory.profilePolicy.title"
  | "memory.profilePolicy.description"
  | "memory.profilePolicy.user"
  | "memory.profilePolicy.project"
  | "memory.profilePolicy.transient"
  | "memory.profilePolicy.situational"
  | "memory.profilePolicy.stable"
  | "memory.profilePolicy.baseMs"
  | "memory.profilePolicy.stepMs"
  | "memory.profilePolicy.maxMs"
  | "memory.profilePolicy.unit"
  | "memory.profilePolicy.hint"
  | "memory.profilePolicy.save"
  | "memory.profilePolicy.saved"
  | "memory.textPolicy.description"
  | "memory.textPolicy.fold"
  | "memory.textPolicy.budget"
  | "memory.textPolicy.budgetHint"
  | "memory.textPolicy.save"
  | "memory.textPolicy.saved"
  | "memory.policy.description"
  | "memory.policy.precheckHistory"
  | "memory.policy.precheckBudget"
  | "memory.policy.recallTopK"
  | "memory.policy.reviewLimit"
  | "memory.policy.reviewScore"
  | "memory.policy.queryLimit"
  | "memory.policy.postactionHistory"
  | "memory.policy.postactionBudget"
  | "memory.policy.extractedPriority"
  | "memory.policy.extractedWeight"
  | "memory.policy.extractedExpiry"
  | "memory.policy.extractedStability"
  | "memory.policy.extractedLevel"
  | "memory.policy.working"
  | "memory.policy.stable"
  | "memory.policy.archival"
  | "memory.policy.expiryRange"
  | "memory.policy.extractedHint"
  | "memory.policy.days"
  | "memory.policy.hours"
  | "memory.policy.minutes"
  | "memory.policy.seconds"
  | "memory.policy.milliseconds"
  | "memory.policy.durationUnit"
  | "memory.policy.userSource"
  | "memory.policy.defaultSource"
  | "memory.policy.scoreHint"
  | "memory.policy.invalidValues"
  | "memory.policy.invalidResponse"
  | "memory.policy.saved"
  | "memory.policy.save"
  | "memory.policy.resetDraft"
  | "memory.ranking.title"
  | "memory.cleanup.title"
  | "memory.projectRetention.title"
  | "memory.projectRetention.description"
  | "memory.projectProfile.title"
  | "memory.projectProfile.description"
  | "memory.projectProfile.inherited"
  | "memory.projectProfile.saved"
  | "memory.projectRetention.select"
  | "memory.projectRetention.reload"
  | "memory.projectRetention.inherited"
  | "memory.projectRetention.overridden"
  | "memory.projectRetention.saved"
  | "memory.projectRetention.save"
  | "memory.projectRetention.inherit"
  | "memory.projectRetention.fillGlobal"
  | "memory.cleanup.description"
  | "memory.cleanup.enabled"
  | "memory.cleanup.scan"
  | "memory.cleanup.profileScan"
  | "memory.cleanup.hotWindow"
  | "memory.cleanup.idle"
  | "memory.cleanup.trash"
  | "memory.cleanup.protection"
  | "memory.cleanup.protectionHint"
  | "memory.cleanup.protectShared"
  | "memory.cleanup.priority"
  | "memory.cleanup.level"
  | "memory.cleanup.session"
  | "memory.cleanup.phase"
  | "memory.cleanup.stable"
  | "memory.cleanup.persistent"
  | "memory.cleanup.unit"
  | "memory.cleanup.days"
  | "memory.cleanup.hours"
  | "memory.cleanup.minutes"
  | "memory.cleanup.seconds"
  | "memory.cleanup.milliseconds"
  | "memory.cleanup.invalid"
  | "memory.cleanup.save"
  | "memory.cleanup.saved"
  | "memory.ranking.description"
  | "memory.ranking.hybrid"
  | "memory.ranking.mmr"
  | "memory.ranking.decay"
  | "memory.ranking.advanced"
  | "memory.ranking.lexical"
  | "memory.ranking.rrf"
  | "memory.ranking.replace"
  | "memory.ranking.dedupe"
  | "memory.ranking.dedupePool"
  | "memory.ranking.replacePool"
  | "memory.ranking.rerank"
  | "memory.ranking.lambda"
  | "memory.ranking.shape"
  | "memory.ranking.scale"
  | "memory.ranking.floor"
  | "memory.ranking.reinforce"
  | "memory.ranking.crossSession"
  | "memory.ranking.scoreHint"
  | "memory.ranking.save"
  | "memory.ranking.saved"
  | "memory.ranking.invalidValues"
  | "memory.resources.loading"
  | "memory.resources.refresh"
  | "memory.resources.promptBundle"
  | "memory.resources.piiLocale"
  | "memory.resources.noiseLocale"
  | "memory.resources.noiseEnabled"
  | "memory.resources.noiseSemanticEnabled"
  | "memory.resources.noiseThreshold"
  | "memory.resources.saveComposition"
  | "memory.resources.fileEditor"
  | "memory.resources.fileEditorDescription"
  | "memory.resources.file"
  | "memory.resources.userOverride"
  | "memory.resources.builtIn"
  | "memory.resources.highRisk"
  | "memory.resources.promptHowTo"
  | "memory.resources.beginPromptEdit"
  | "memory.resources.builtInSource"
  | "memory.resources.effectiveEditor"
  | "memory.resources.saveFile"
  | "memory.resources.restoreBuiltIn"
  | "memory.resources.piiTester"
  | "memory.resources.piiTesterDescription"
  | "memory.resources.piiActiveBundleTest"
  | "memory.resources.piiFormulaTester"
  | "memory.resources.piiFormulaTesterDescription"
  | "memory.resources.piiSampleText"
  | "memory.resources.piiPattern"
  | "memory.resources.piiCondition"
  | "memory.resources.piiReplacement"
  | "memory.resources.piiCandidates"
  | "memory.resources.piiAccepted"
  | "memory.resources.piiRejected"
  | "memory.resources.piiCaptures"
  | "memory.resources.piiNoCandidates"
  | "memory.resources.noiseTester"
  | "memory.resources.noiseTesterDescription"
  | "memory.resources.role"
  | "memory.resources.runTest"
  | "memory.resources.noMatch"
  | "memory.resources.noiseMatched"
  | "memory.resources.confirmPurposeTitle"
  | "memory.resources.confirmModificationTitle"
  | "memory.resources.confirmModificationDescription"
  | "memory.resources.confirmPurpose"
  | "memory.resources.unlockEditor"
  | "memory.resources.promptPurpose.precheckL1"
  | "memory.resources.promptPurpose.precheckL2"
  | "memory.resources.promptPurpose.postactionL1"
  | "memory.resources.promptPurpose.postactionL2"
  | "memory.resources.promptPurpose.profileInstruction"
  | "memory.resources.promptPurpose.unknown"
  | "memory.resources.overrideDirectory"
  | "memory.resources.promptTab"
  | "memory.resources.piiTab"
  | "memory.resources.noiseTab"
  | "memory.resources.unlocked"
  | "memory.resources.locked"
  | "memory.resources.guardedEdit"
  | "memory.resources.currentEffective"
  | "memory.resources.composition"
  | "memory.resources.compositionDescription"
  | "memory.resources.piiPurpose"
  | "memory.resources.noisePurpose"
  | "memory.resources.saveConfirmTitle"
  | "memory.resources.resetConfirmTitle"
  | "memory.resources.saveConfirmDescription"
  | "memory.resources.resetConfirmDescription"
  | "memory.resources.saveConfirmAction"
  | "memory.resources.resetConfirmAction"
  | "memory.resources.userPromptOnly"
  | "memory.maintenance.title"
  | "memory.maintenance.description"
  | "memory.maintenance.invalidResponse"
  | "memory.maintenance.readFailed"
  | "memory.maintenance.runNow"
  | "memory.lexical.title"
  | "memory.lexical.description"
  | "memory.lexical.documents"
  | "memory.lexical.rows"
  | "memory.lexical.rebuild"
  | "memory.lexical.confirmTitle"
  | "memory.lexical.confirmDescription"
  | "memory.lexical.completed"
  | "memory.lexical.countsNote"
  | "memory.maintenance.previewCleanup"
  | "memory.maintenance.cleanupPreviewTitle"
  | "memory.maintenance.cleanupPreviewDescription"
  | "memory.maintenance.cleanupImpact"
  | "memory.maintenance.cleanupRecordIds"
  | "memory.maintenance.status"
  | "memory.maintenance.statusDescription"
  | "memory.maintenance.worker"
  | "memory.maintenance.profileExpiryWorker"
  | "memory.maintenance.running"
  | "memory.maintenance.enabled"
  | "memory.maintenance.disabled"
  | "memory.maintenance.interval"
  | "memory.maintenance.profileExpiryInterval"
  | "memory.maintenance.vectorTitle"
  | "memory.maintenance.vectorDescription"
  | "memory.maintenance.vectorConfigured"
  | "memory.maintenance.configured"
  | "memory.maintenance.notConfigured"
  | "memory.maintenance.vectorDimension"
  | "memory.maintenance.activeMemories"
  | "memory.maintenance.recycleBinMemories"
  | "memory.maintenance.currentVectors"
  | "memory.maintenance.currentRecycleBinVectors"
  | "memory.maintenance.pendingAnalysisJobs"
  | "memory.maintenance.vectorRebuildRequired"
  | "memory.maintenance.vectorRouteUnavailable"
  | "memory.maintenance.vectorCompatible"
  | "memory.maintenance.vectorRebuildCounts"
  | "memory.maintenance.previewVectorRebuild"
  | "memory.maintenance.confirmVectorRebuildTitle"
  | "memory.maintenance.confirmVectorRebuildDescription"
  | "memory.maintenance.rebuildVectors"
  | "memory.maintenance.pendingJobs"
  | "memory.maintenance.recycleBatches"
  | "memory.maintenance.hotWindow"
  | "memory.maintenance.idleWindow"
  | "memory.maintenance.trashWindow"
  | "memory.maintenance.protectionFloor"
  | "memory.maintenance.limits"
  | "memory.maintenance.profileExpiryLimit"
  | "memory.maintenance.memoryRecycleLimit"
  | "memory.maintenance.coldScanLimit"
  | "memory.maintenance.coldJobLimit"
  | "memory.maintenance.idleSessionLimit"
  | "memory.maintenance.trashPurgeLimit"
  | "memory.maintenance.vectorConsistency"
  | "memory.maintenance.lastRun"
  | "memory.maintenance.profileExpiryLastRun"
  | "memory.maintenance.profileExpiryLastResult"
  | "memory.maintenance.never"
  | "memory.maintenance.lastResult"
  | "memory.maintenance.recycleBin"
  | "memory.maintenance.recycleDescription"
  | "memory.maintenance.projectFilter"
  | "memory.maintenance.allProjects"
  | "memory.maintenance.emptyRecycleBin"
  | "memory.maintenance.confirmPurgeTitle"
  | "memory.maintenance.confirmRestoreTitle"
  | "memory.maintenance.confirmPurgeDescription"
  | "memory.maintenance.confirmRestoreDescription"
  | "memory.maintenance.purge"
  | "memory.maintenance.restore"
  | "memory.maintenance.turns"
  | "memory.maintenance.memories"
  | "memory.maintenance.profiles"
  | "memory.maintenance.evidence"
  | "memory.recycleRecords.inspect"
  | "memory.recycleRecords.description"
  | "memory.recycleRecords.reload"
  | "memory.recycleRecords.total"
  | "memory.recycleRecords.failed"
  | "memory.recycleRecords.empty"
  | "memory.recycleRecords.previous"
  | "memory.recycleRecords.next"
  | "memory.recycleRecords.session"
  | "memory.recycleRecords.savedStatus"
  | "memory.recycleRecords.sourceTurns"
  | "memory.recycleRecords.userProfile"
  | "memory.recycleRecords.projectProfile"
  | "memory.recycleRecords.support"
  | "memory.recycleRecords.rebuttal"
  | "memory.maintenance.expiresAt"
  | "memory.maintenance.reasonTerminal"
  | "memory.maintenance.reasonColdTurn"
  | "memory.maintenance.reasonIdle"
  | "memory.maintenance.reasonManualSession"
  | "memory.maintenance.reasonManualTurn"
  | "memory.maintenance.manualSources"
  | "vmm.experience.deployment.native"
  | "vmm.experience.deployment.disabled"
  | "vmm.experience.deployment.external"
  | "vmm.experience.setup.notConfigured"
  | "vmm.experience.setup.checking"
  | "vmm.experience.setup.ready"
  | "vmm.experience.setup.incomplete"
  | "vmm.experience.setup.blocked"
  | "vmm.experience.lifecycle.stopped"
  | "vmm.experience.lifecycle.running"
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
  | "vmm.nativeProjects.title"
  | "vmm.nativeProjects.description"
  | "vmm.nativeProjects.refresh"
  | "vmm.nativeProjects.running"
  | "vmm.nativeProjects.createTitle"
  | "vmm.nativeProjects.createDescription"
  | "vmm.nativeProjects.selectVulcanProject"
  | "vmm.nativeProjects.projectName"
  | "vmm.nativeProjects.create"
  | "vmm.nativeProjects.bindingsTitle"
  | "vmm.nativeProjects.bindingsDescription"
  | "vmm.nativeProjects.unbound"
  | "vmm.nativeProjects.saveBinding"
  | "vmm.nativeProjects.linkedRecall"
  | "vmm.nativeProjects.catalogTitle"
  | "vmm.nativeProjects.catalogDescription"
  | "vmm.nativeProjects.rename"
  | "vmm.nativeProjects.clear"
  | "vmm.nativeProjects.delete"
  | "vmm.nativeProjects.protected"
  | "vmm.nativeProjects.migrateTitle"
  | "vmm.nativeProjects.migrateDescription"
  | "vmm.nativeProjects.source"
  | "vmm.nativeProjects.target"
  | "vmm.nativeProjects.previewMigration"
  | "vmm.nativeProjects.preview.migration"
  | "vmm.nativeProjects.preview.clear"
  | "vmm.nativeProjects.preview.delete"
  | "vmm.nativeProjects.previewCounts"
  | "vmm.nativeProjects.previewMigrationCounts"
  | "vmm.nativeProjects.previewConflicts"
  | "vmm.nativeProjects.previewRecords"
  | "vmm.nativeProjects.recordKind.session"
  | "vmm.nativeProjects.recordKind.turn"
  | "vmm.nativeProjects.recordKind.memory"
  | "vmm.nativeProjects.recordKind.project_profile"
  | "vmm.nativeProjects.recordSession"
  | "vmm.nativeProjects.previewOwners"
  | "vmm.nativeProjects.previewLinks"
  | "vmm.nativeProjects.confirmRenameMigration"
  | "vmm.nativeProjects.confirm"
  | "vmm.setup.title"
  | "vmm.setup.description"
  | "vmm.setup.stepLabel"
  | "vmm.setup.previous"
  | "vmm.setup.next"
  | "vmm.setup.mode.title"
  | "vmm.setup.mode.description"
  | "vmm.setup.native.title"
  | "vmm.setup.native.description"
  | "vmm.setup.external.title"
  | "vmm.setup.external.description"
  | "vmm.setup.inference.title"
  | "vmm.setup.inference.description"
  | "vmm.setup.inference.required"
  | "vmm.setup.inference.openCredentials"
  | "vmm.setup.binding.title"
  | "vmm.setup.binding.description"
  | "vmm.setup.binding.fixed"
  | "vmm.setup.binding.fixedDescription"
  | "vmm.setup.binding.separate"
  | "vmm.setup.binding.separateDescription"
  | "vmm.setup.binding.defaultProject"
  | "vmm.setup.binding.prefix"
  | "vmm.setup.binding.unboundProjects"
  | "vmm.setup.binding.none"
  | "vmm.setup.binding.create"
  | "vmm.setup.binding.existing"
  | "vmm.setup.binding.selectProject"
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
  | "vmm.runtime.mode.external"
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
  | "runtime.thinkingTokenCount"
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
  | "settings.providerDescription.commandcode"
  | "settings.providerDescription.tavily"
  | "settings.providerDescription.openai"
  | "settings.providerDescription.anthropic"
  | "settings.providerDescription.google"
  | "settings.providerDescription.agentspace"
  | "settings.providerDescription.tokenrhythm"
  | "settings.providerName.tokenrhythm"
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
  | "settings.modelBasics"
  | "settings.providerRetry"
  | "settings.providerUsage"
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
  | "settings.visualLab"
  | "settings.gitCommit"
  | "settings.aboutLicenses"
  | "settings.archivedRestore"
  | "settings.modelProviders.description"
  | "settings.credentialManagement.description"
  | "settings.modelList.description"
  | "settings.modelBasics.description"
  | "settings.providerRetry.description"
  | "settings.providerUsage.description"
  | "settings.aiVendors.description"
  | "settings.aiWebSearch.description"
  | "settings.aiWebExtract.description"
  | "settings.aiUnderstandImage.description"
  | "settings.aiUnderstandVideo.description"
  | "settings.aiUnderstandAudio.description"
  | "settings.aiImageGeneration.description"
  | "settings.aiImageEdit.description"
  | "settings.aiVideoGeneration.description"
  | "settings.aiVideoEdit.description"
  | "settings.aiVideoExtend.description"
  | "settings.aiSpeechSynthesis.description"
  | "settings.aiSpeechTranscribe.description"
  | "settings.aiMusicGeneration.description"
  | "settings.aiMusicCover.description"
  | "settings.mcpServers.description"
  | "settings.runtimeDiagnostics.description"
  | "settings.visualLab.description"
  | "settings.diagnostics.logs"
  | "settings.diagnostics.startupStatus"
  | "settings.diagnostics.waitingStatus"
  | "settings.diagnostics.providerRequestTrace"
  | "settings.diagnostics.providerRequestTrace.completeRequests"
  | "settings.diagnostics.providerRequestTrace.openDirectory"
  | "settings.diagnostics.providerRequestTrace.clearDirectory"
  | "settings.diagnostics.providerRequestTrace.clearConfirm"
  | "settings.diagnostics.providerRequestTrace.clearSucceeded"
  | "settings.diagnostics.providerRequestTrace.clearEmpty"
  | "settings.diagnostics.providerRequestTrace.actionFailed"
  | "settings.diagnostics.providerRequestTrace.description"
  | "settings.diagnostics.providerRequestTrace.recordedData"
  | "settings.diagnostics.providerRequestTrace.recordedDataHelp"
  | "settings.diagnostics.providerRequestTrace.retention"
  | "settings.diagnostics.providerRequestTrace.retentionHelp"
  | "settings.diagnostics.providerRequestTrace.privacy"
  | "settings.diagnostics.providerRequestTrace.privacyHelp"
  | "settings.diagnostics.providerRequestTrace.disabled"
  | "settings.diagnostics.providerRequestTrace.enabled"
  | "settings.diagnostics.providerRequestTrace.saveFailed"
  | "settings.diagnostics.providerRequestTrace.switch"
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
  | "settings.customModels.enabled"
  | "settings.customModels.disabled"
  | "settings.customModels.protocolPreference"
  | "settings.customModels.protocolPreferenceHint"
  | "settings.customModels.protocolAuto"
  | "settings.customModels.protocolExplicit"
  | "settings.customModels.protocolSelect"
  | "settings.customModels.saveProtocolPreference"
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
  | "settings.customProviders.protocols"
  | "settings.customProviders.protocolsHint"
  | "settings.customProviders.protocolCollectionRequired"
  | "settings.customProviders.protocolCollectionDuplicate"
  | "settings.customProviders.routeDrafts"
  | "settings.customProviders.routeDraftsHint"
  | "settings.customProviders.routeDraftFor"
  | "settings.customProviders.routeBaseUrl"
  | "settings.customProviders.routeBaseUrlPlaceholder"
  | "settings.customProviders.routeEndpointPath"
  | "settings.customProviders.routeAuthentication"
  | "settings.customProviders.routeDiscoveryMode"
  | "settings.customProviders.routeDiscoveryPath"
  | "settings.customProviders.routeHeaders"
  | "settings.customProviders.routeInherit"
  | "settings.customProviders.advancedOptions"
  | "settings.customProviders.sessionHeader"
  | "settings.customProviders.sessionHeaderHint"
  | "settings.customProviders.codexCapabilities"
  | "settings.customProviders.codexRemoteCompaction"
  | "settings.customProviders.codexRemoteCompactionHint"
  | "settings.customProviders.codexWebsocket"
  | "settings.customProviders.codexWebsocketHint"
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
  | "settings.customModels.toolSchemaProjection"
  | "settings.customModels.toolSchemaProjectionHint"
  | "settings.customModels.toolSchemaProjectionLightweight"
  | "settings.customModels.toolSchemaProjectionPassthrough"
  | "settings.customModels.toolSchemaProjectionStrict"
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
  | "settings.contextBasics.title"
  | "settings.mediaDelivery.title"
  | "settings.mediaDelivery.description"
  | "settings.mediaDelivery.uploadRule"
  | "settings.mediaDelivery.historyRule"
  | "settings.mediaDelivery.conversation"
  | "settings.mediaDelivery.extension"
  | "settings.mediaDelivery.kind"
  | "settings.mediaDelivery.image"
  | "settings.mediaDelivery.audio"
  | "settings.mediaDelivery.video"
  | "settings.mediaDelivery.file"
  | "settings.mediaDelivery.withUpload"
  | "settings.mediaDelivery.withoutUpload"
  | "settings.mediaDelivery.aggregate"
  | "settings.mediaDelivery.rawMaximum"
  | "settings.mediaDelivery.totalMaximum"
  | "settings.mediaDelivery.failureRule"
  | "settings.mediaDelivery.invalidNumber"
  | "settings.mediaDelivery.invalidLimit"
  | "settings.mediaDelivery.invalidFraction"
  | "settings.mediaDelivery.conflict"
  | "settings.mediaDelivery.save"
  | "settings.mediaDelivery.reset"
  | "settings.mediaDelivery.reload"
  | "settings.contextBasics.description"
  | "settings.contextBasics.loading"
  | "settings.contextBasics.saveSystemDefault"
  | "settings.contextCompactionNodes.title"
  | "settings.contextCompactionNodes.description"
  | "settings.contextCompactionNodes.warning"
  | "settings.contextCompactionNodes.node1"
  | "settings.contextCompactionNodes.node2"
  | "settings.contextCompactionNodes.node3"
  | "settings.contextCompactionNodes.node4"
  | "settings.contextCompactionNodes.node5"
  | "settings.contextCompactionNodes.invalidPositive"
  | "settings.contextCompactionNodes.invalidAscending"
  | "settings.contextCompactionNodes.save"
  | "settings.usageRefresh.title"
  | "settings.usageRefresh.description"
  | "settings.usageRefresh.minute"
  | "settings.usageRefresh.minutes"
  | "settings.usageRefresh.hour"
  | "settings.credentials.selectProvider"
  | "settings.credentials.categoryAria"
  | "settings.credentials.collapse"
  | "settings.credentials.expand"
  | "settings.credentials.connecting"
  | "settings.credentials.connectingDescription"
  | "settings.credentials.emptyProvider"
  | "settings.credentials.emptyProviderDescription"
  | "settings.credentials.emptyAll"
  | "settings.credentials.emptyAllDescription"
  | "settings.credentials.refreshUsage"
  | "settings.credentials.syncModels"
  | "settings.credentials.enable"
  | "settings.credentials.disable"
  | "settings.credentials.disabledStatus"
  | "settings.credentials.currentDefault"
  | "settings.credentials.setDefault"
  | "settings.credentials.edit"
  | "settings.credentials.remove"
  | "settings.credentials.deleteDefaultMissingReplacement"
  | "settings.credentials.deleteTitle"
  | "settings.credentials.deleteDescription"
  | "settings.credentials.replacementDefault"
  | "settings.credentials.cancel"
  | "settings.credentials.deleting"
  | "settings.credentials.confirmDelete"
  | "settings.credentials.usageUnsupported"
  | "settings.credentials.usageNotQueried"
  | "settings.credentials.usageUnavailable"
  | "settings.credentials.usageInvalid"
  | "settings.credentials.noQuotaWindows"
  | "settings.credentials.quotaWindow"
  | "settings.credentials.remaining"
  | "settings.credentials.used"
  | "settings.credentials.notIncluded"
  | "settings.credentials.unlimited"
  | "settings.credentials.unknown"
  | "settings.credentials.connectionIncomplete"
  | "settings.credentials.editToolTitle"
  | "settings.credentials.editModelTitle"
  | "settings.credentials.addToolTitle"
  | "settings.credentials.addModelTitle"
  | "settings.credentials.toolDescription"
  | "settings.credentials.modelDescription"
  | "settings.credentials.provider"
  | "settings.credentials.serviceEntry"
  | "settings.credentials.autoRegion"
  | "settings.credentials.credentialType"
  | "settings.credentials.displayName"
  | "settings.credentials.replaceToken"
  | "settings.credentials.apiToken"
  | "settings.credentials.managementKey"
  | "settings.credentials.managementKeyPlaceholder"
  | "settings.credentials.managementKeyHint"
  | "settings.credentials.removeManagementKey"
  | "settings.credentials.oauthClientSecretOptional"
  | "settings.credentials.authorizationStatus"
  | "settings.credentials.authorizationCode"
  | "settings.credentials.cancelAuthorization"
  | "settings.credentials.waitBrowser"
  | "settings.credentials.saveSyncing"
  | "settings.credentials.reauthorizeAndSave"
  | "settings.credentials.openBrowserAuthorize"
  | "settings.credentials.saveSync"
  | "settings.credentials.oauth.waitingAuthorization"
  | "settings.credentials.oauth.waitingCallback"
  | "settings.credentials.oauth.finalizing"
  | "settings.credentials.oauth.succeeded"
  | "settings.credentials.oauth.failed"
  | "settings.credentials.oauth.cancelled"
  | "settings.providers.managementTitle"
  | "settings.providers.managementDescription"
  | "settings.providers.system"
  | "settings.providers.functional"
  | "settings.providers.custom"
  | "settings.providers.catalog"
  | "settings.providers.isolatedCount"
  | "settings.providers.isolatedDescription"
  | "settings.providers.removeIsolated"
  | "settings.providers.diagnosticsOmitted"
  | "settings.providers.removeIsolatedTitle"
  | "settings.providers.removeIsolatedDescription"
  | "settings.providers.permanentlyDelete"
  | "settings.providers.customTitle"
  | "settings.providers.addProvider"
  | "settings.providers.loading"
  | "settings.providers.emptyCustom"
  | "settings.providers.table.provider"
  | "settings.providers.table.endpoint"
  | "settings.providers.table.resources"
  | "settings.providers.table.status"
  | "settings.providers.table.priority"
  | "settings.providers.table.actions"
  | "settings.providers.table.details"
  | "settings.providers.table.configurationStatus"
  | "settings.providers.table.operation"
  | "settings.providers.table.protocolUnavailable"
  | "settings.providers.credentialCount"
  | "settings.providers.configured"
  | "settings.providers.notConfigured"
  | "settings.providers.addCredentialFor"
  | "settings.providers.editFor"
  | "settings.providers.removeFor"
  | "settings.providers.priorityAria"
  | "settings.providers.savePriority"
  | "settings.contextWindowPolicy.effectivePercent"
  | "settings.contextWindowPolicy.autoCompactPercent"
  | "settings.contextWindowPolicy.autoCompactTokenLimit"
  | "settings.contextWindowPolicy.scope"
  | "settings.contextWindowPolicy.scopeInherit"
  | "settings.contextWindowPolicy.scopeTotal"
  | "settings.contextWindowPolicy.scopeBodyAfterPrefix"
  | "settings.contextWindowPolicy.fallbackBuffer"
  | "settings.contextWindowPolicy.clear"
  | "settings.models.conversationUse"
  | "settings.models.multimodalUnderstandingCandidate"
  | "settings.models.multimodalUnderstandingUnavailable"
  | "settings.providerTools.mediaBackendMode"
  | "settings.providerTools.selectBackendMode"
  | "settings.providerTools.providerToolMode"
  | "settings.providerTools.modelReplacementMode"
  | "settings.providerTools.reasoningEffort"
  | "settings.providerTools.modelDefaultReasoning"
  | "settings.models.internalUse"
  | "settings.models.loadingCatalog"
  | "settings.models.provider"
  | "settings.models.liveModels"
  | "settings.models.noProviderSelected"
  | "settings.models.catalogSummary"
  | "settings.models.memoryOnly"
  | "settings.models.emptyCatalog"
  | "settings.models.availableCredentials"
  | "settings.models.additionalConditions"
  | "settings.models.routeEffectiveProtocol"
  | "settings.models.routeAvailableProtocols"
  | "settings.models.routeBindingSource"
  | "settings.models.routeBindingRule"
  | "settings.models.routeManifestRevision"
  | "settings.models.routeUnresolved"
  | "settings.models.routeUnavailableReason"
  | "settings.models.routeNoRule"
  | "settings.models.blockedRouteDiagnostics"
  | "settings.models.codexBehavior"
  | "settings.models.requiredRuntimePrimitive"
  | "settings.models.collapseProviders"
  | "settings.models.expandProviders"
  | "settings.models.searchLabel"
  | "settings.models.searchPlaceholder"
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
  | "settings.inference.rerankEnabled"
  | "settings.inference.rerankDisabled"
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
  | "settings.inference.reasoningDisabled"
  | "settings.inference.reasoningAdaptive"
  | "settings.inference.reasoningMinimal"
  | "settings.inference.reasoningXHigh"
  | "settings.inference.reasoningUltra"
  | "settings.inference.embeddingTask"
  | "settings.inference.embeddingDimensions"
  | "settings.inference.embeddingOutputKind"
  | "settings.inference.embeddingEncoding"
  | "settings.inference.rerankTopN"
  | "settings.inference.rerankTruncation"
  | "settings.inference.rerankReturnContent"
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
  | "settings.providerTools.videoExtend"
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
  | "skills.label.package"
  | "skills.label.plugin"
  | "skills.catalogSummary"
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
  | "workspaceToolPanel.category"
  | "skills.navigation.builtins"
  | "skills.navigation.externalPlugins"
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
    "ui.close": "关闭",
    "ui.loading": "加载中",
    "settings.modelExperience.invalidValue": "当前值不在声明能力范围内",
    "settings.modelExperience.extensionSelectionHint": "选择允许 AI 查询并按需调用的原生扩展模型。每个模型独立选择账号。",
    "settings.modelExperience.retainedEmbeddingInvalid": "保留的向量维度或批量值无效，请切回向量模式修正后保存。",
    "gitRepository.list": "Git 仓库",
    "gitRepository.primary": "主仓库",
    "gitRepository.nested": "嵌套仓库",
    "gitRepository.branchUnknown": "分支待确认",
    "gitRepository.changesUnknown": "变更待确认",
    "gitRepository.changedFiles": "{count} 个变更文件",
    "gitRepository.refreshing": "正在刷新",
    "gitRepository.readFailed": "读取失败",
    "gitRepository.busy": "操作进行中",
    "gitRepository.loading": "正在读取仓库状态…",
    "gitRepository.unavailable": "仓库状态尚未取得",
    "gitRepository.retryReview": "打开审查并重试",
    "projectOverview.conversations": "{count} 个会话",
    "projectOverview.pinned": "已置顶",
    "projectOverview.issues": "需要处理",
    "projectOverview.memoryLoading": "正在读取记忆配置…",
    "projectOverview.memoryUnknown": "记忆配置尚未取得",
    "projectOverview.native": "本地原生",
    "projectOverview.external": "外置服务",
    "projectOverview.enabled": "配置已启用",
    "projectOverview.disabled": "配置已关闭",
    "projectOverview.override": "项目覆盖",
    "projectOverview.inherited": "继承全局",
    "projectOverview.unsaved": "存在未保存修改",
    "projectOverview.missingOwner": "记忆绑定目标 #{id} 不在当前目录中",
    "projectOverview.ownerUnknown": "绑定目标不在目录中",
    "projectOverview.nativeBinding": "{name} (#{id}) · {count} 个连带召回项目",
    "projectOverview.unbound": "尚未绑定记忆项目",
    "projectOverview.bindingUnknown": "绑定信息尚未取得",
    "projectOverview.externalBinding": "外置记忆项目 #{id}",
    "projectOverview.externalUnbound": "尚未绑定外置记忆项目",
    "projectOverview.precheck": "前置注入",
    "projectOverview.writeback": "回合写回",
    "projectOverview.on": "开启",
    "projectOverview.off": "关闭",
    "projectOverview.session": "当前会话",
    "projectOverview.scopeReady": "记忆作用域已就绪",
    "projectOverview.scopeNotReady": "记忆作用域未就绪",
    "projectOverview.precheckReady": "前置已就绪",
    "projectOverview.precheckNotReady": "前置未就绪",
    "projectOverview.postactionReady": "后置已就绪",
    "projectOverview.postactionNotReady": "后置未就绪",
    "projectOverview.runtimeUnknown": "此项目暂无当前会话运行快照",
    "projectOverview.memorySettings": "记忆设置",
    "projectOverview.codeSettings": "代码语义索引设置",
    "projectOverview.knowledgeSettings": "知识库设置",
    "projectOverview.skillSettings": "技能设置",
    "projectOverview.indexDisabled": "代码语义索引已关闭",
    "projectOverview.queryReady": "代码语义索引可检索",
    "projectOverview.queryUnavailable": "代码语义索引暂不可检索",
    "projectOverview.indexUnknown": "索引状态尚未取得",
    "projectOverview.watching": "正在监听源码",
    "projectOverview.notWatching": "未监听源码",
    "projectOverview.pending": "源码更新待同步",
    "projectOverview.files": "{count} 个文件",
    "projectOverview.routesUnavailable": "模型路由尚未就绪",
    "projectOverview.knowledgeCount": "已授权 {total} 个知识库 · 默认启用 {enabled} 个",
    "projectOverview.knowledgeFailed": "知识库绑定读取失败",
    "projectOverview.knowledgeLoading": "正在读取知识库绑定…",
    "projectOverview.skillsLoading": "正在读取技能覆盖…",
    "projectOverview.skillsUnknown": "技能覆盖尚未取得",
    "projectOverview.skillsCount": "{count} 项项目技能覆盖",
    "projectOverview.skillsInherited": "技能配置继承全局",
    "projectOverview.maintenance": "附件缓存按需统计，清理前先预览影响。",
    "projectOverview.attachments": "查看与清理附件",
    "projectOverview.details": "项目目录与存储详情",
    "projectOverview.path": "项目目录",
    "projectOverview.copyPath": "复制项目路径",
    "projectOverview.copy": "复制路径",
    "projectOverview.copied": "已复制",
    "projectOverview.copyFailed": "复制失败，请选择路径文本复制",
    "projectOverview.attachmentPath": "附件目录",
    "projectOverview.id": "项目标识",
    "settings.licenses.title": "关于与许可证",
    "settings.licenses.types": "许可证类型",
    "settings.licenses.dependencies": "依赖",
    "settings.licenses.scope": "当前目录包含 Rust 桌面应用依赖；前端与 Lua 依赖尚未纳入此目录。",
    "settings.licenses.search": "搜索许可证、依赖、版本或来源",
    "settings.licenses.reload": "重新加载",
    "settings.licenses.loadError": "许可证资源读取失败",
    "settings.licenses.loading": "正在加载许可证目录…",
    "settings.licenses.emptySearch": "没有匹配的许可证或依赖。",
    "settings.licenses.unknown": "未知许可证",
    "settings.licenses.variants": "份原始正文",
    "settings.licenses.dependency": "依赖名称",
    "settings.licenses.version": "版本",
    "settings.licenses.source": "来源",
    "settings.licenses.body": "正文",
    "settings.licenses.view": "查看",
    "settings.licenses.previous": "上一页依赖",
    "settings.licenses.next": "下一页依赖",
    "settings.licenses.variant": "选择原始正文",
    "settings.licenses.variantHint": "同类型许可证可能包含不同版权声明；每份正文保留各自实际覆盖的依赖。",
    "settings.licenses.retry": "重试读取",
    "settings.licenses.loadingBody": "正在读取原始正文…",
    "settings.modelExperience.retryLoad": "重新加载设置",
    "settings.modelExperience.context": "上下文与压缩",
    "settings.modelExperience.conversation": "会话默认",
    "settings.modelExperience.nodes": "压缩窗口节点",
    "settings.modelExperience.retry": "请求与重试",
    "settings.modelExperience.media": "媒体交付",
    "settings.modelExperience.usage": "用量刷新",
    "settings.modelExperience.scopeHint": "保存仅修改系统默认层；供应商、模型及会话覆盖仍独立生效。",
    "settings.modelExperience.inheritWindow": "继承内置默认：原始模型窗口的 95%",
    "settings.modelExperience.inheritCompact": "继承内置默认：原始模型窗口的 90%",
    "settings.modelExperience.inheritToken": "内置默认未设固定 Token 上限，按比例计算",
    "settings.modelExperience.inheritScope": "继承内置默认：总用量",
    "settings.modelExperience.inheritBuffer": "继承内置默认：0 Token；仅在存在回退提示词时使用",
    "settings.modelExperience.savingSystem": "正在保存系统默认…",
    "settings.modelExperience.dirtySystem": "系统默认有未保存更改",
    "settings.modelExperience.savedSystem": "当前系统默认已保存",
    "settings.modelExperience.savingNodes": "正在保存节点…",
    "settings.modelExperience.dirtyNodes": "压缩窗口节点有未保存更改",
    "settings.modelExperience.savedNodes": "当前节点配置已保存",
    "settings.modelExperience.nodeIds": "固定节点标识",
    "settings.modelExperience.statistics": "查看统计与定价",
    "settings.modelExperience.searchCustom": "搜索自定义供应商",
    "settings.modelExperience.searchProvider": "搜索供应商",
    "settings.modelExperience.searchPlaceholder": "搜索供应商名称或标识",
    "settings.modelExperience.noProviders": "没有匹配的供应商",
    "settings.modelExperience.providerDetails": "接入与协议详情",
    "settings.modelExperience.usageFilter": "筛选模型用途",
    "settings.modelExperience.allUsages": "全部用途",
    "settings.modelExperience.conversationEnabled": "会话使用已开启",
    "settings.modelExperience.internalEnabled": "插件调用已开启",
    "settings.modelExperience.multimodalEnabled": "多模态理解候选已开启",
    "settings.modelExperience.capabilityFilter": "筛选模型能力",
    "settings.modelExperience.allCapabilities": "全部能力",
    "settings.modelExperience.routes": "接入详情 · {count} 条路线",
    "settings.modelExperience.modelColumn": "模型",
    "settings.modelExperience.understandingUse": "多模态理解",
    "settings.modelExperience.accessButton": "接入",
    "settings.modelExperience.candidateExplanation": "允许作为图片、视频或音频理解的候选；默认模型在对应理解设置中选择。",
    "settings.modelExperience.routeSettings": "附加条件",
    "settings.modelExperience.bindingSource": "绑定来源",
    "settings.modelExperience.routeAccounts": "该路线的凭据账号",
    "settings.modelExperience.available": "可用",
    "settings.modelExperience.accountNotAdmitted": "账号未准入此模型",
    "settings.modelExperience.internalPurposes": "调用用途：{purposes}",
    "settings.modelExperience.candidateHint": "理解候选已开启；默认模型在对应理解设置中独立选择。",
    "settings.modelExperience.noConversation": "无会话响应能力",
    "settings.modelExperience.unavailableCandidates": "不可用的理解候选",
    "settings.modelExperience.disableCandidate": "关闭此候选",
    "settings.modelExperience.curationModel": "整理模型",
    "settings.modelExperience.embeddingModel": "向量模型",
    "settings.modelExperience.modelSearch": "搜索模型、供应商或账号",
    "settings.modelExperience.curationNoMatch": "没有匹配的整理模型",
    "settings.modelExperience.modelNoMatch": "没有匹配的模型",
    "knowledge.experience.targetChanged": "目标知识库已变化，请取消后重新确认。",
    "knowledge.experience.createTitle": "创建资料库",
    "knowledge.experience.retrievalMode": "检索模式",
    "knowledge.experience.rawMode": "原始库 BM25（无模型成本）",
    "knowledge.experience.displayMode": "LLM 整理显示库 + BM25",
    "knowledge.experience.hybridMode": "LLM 整理显示库 + BM25 + 向量",
    "knowledge.experience.reasoning": "思考级别",
    "knowledge.experience.reasoningPlaceholder": "选择模型声明的思考级别",
    "knowledge.experience.reasoningHint": "当前模型声明的级别会随资料库配置保存，并用于整理请求。",
    "knowledge.experience.dimensions": "向量维度",
    "knowledge.experience.dimensionsPlaceholder": "选择模型声明的维度",
    "knowledge.experience.dimensionsInput": "输入模型支持的维度",
    "knowledge.experience.dimensionsInvalid": "维度不在当前模型能力范围内",
    "knowledge.experience.batch": "Embedding 批量（官方上限：{limit}）",
    "knowledge.experience.batchFallback": "未声明，使用默认值 10",
    "knowledge.experience.compatibility": "同维度兼容性确认（可选）",
    "knowledge.experience.compatibilityPlaceholder": "模型更换但维度相同，确认可复用现有索引时填写原因",
    "knowledge.experience.compatibilityHint": "填写后会保留旧索引并显示风险；维度不一致时仍会自动关闭向量检索。",
    "knowledge.experience.compatibilityActive": "已人工确认同维度模型兼容，当前复用旧向量索引：{reason}",
    "knowledge.experience.browseCorpus": "浏览内容层",
    "knowledge.experience.rawLibrary": "原始库",
    "knowledge.experience.displayLibrary": "显示库",
    "knowledge.experience.searchLayer": "检索层",
    "knowledge.experience.rawSearch": "原始库 BM25",
    "knowledge.experience.displaySearch": "显示库 BM25",
    "knowledge.experience.hybridSearch": "显示库混合检索",
    "knowledge.experience.viewProgress": "查看生成进度",
    "knowledge.experience.resume": "继续生成",
    "knowledge.experience.displayReady": "显示库已完成",
    "knowledge.experience.regenerate": "重新生成",
    "knowledge.experience.buildDisplay": "生成显示库",
    "knowledge.experience.buildVectors": "生成向量",
    "knowledge.experience.saveSettings": "保存资料库配置",
    "knowledge.experience.configure": "配置",
    "knowledge.experience.settingsTitle": "资料库模型与检索配置",
    "knowledge.experience.displayOperation": "生成资料库显示层",
    "knowledge.experience.vectorOperation": "生成资料库向量索引",
    "knowledge.experience.background": "后台运行",
    "knowledge.experience.curating": "整理显示库",
    "knowledge.experience.curatingNow": "正在整理显示库",
    "knowledge.experience.embeddingNow": "正在生成向量索引",
    "knowledge.experience.curatingProgress": "正在整理显示库 {done} / {total}（{percent}%）",
    "knowledge.experience.embeddingProgress": "正在生成向量 {done} / {total}（{percent}%）",
    "knowledge.experience.cancelling": "正在请求取消",
    "knowledge.experience.failed": "操作失败",
    "knowledge.experience.cancelled": "操作已取消",
    "knowledge.experience.completed": "操作完成",
    "knowledge.experience.running": "正在执行",
    "knowledge.experience.waiting": "等待操作",
    "knowledge.experience.processing": "处理中",
    "knowledge.experience.items": "{done} / {total} 项",
    "knowledge.experience.llmRequests": "LLM 请求",
    "knowledge.experience.embeddingRequests": "向量请求",
    "knowledge.experience.retries": "重试 {count}",
    "knowledge.experience.generated": "生成 {count}",
    "knowledge.experience.inputTokens": "输入 Token",
    "knowledge.experience.outputTokens": "输出 Token",
    "knowledge.experience.cumulative": "累计",
    "knowledge.experience.cacheTokens": "缓存读 / 写",
    "knowledge.experience.currentRequest": "当前请求",
    "knowledge.experience.noRequest": "尚未发起请求",
    "knowledge.experience.requestTokens": "输入 {input} / 输出 {output}",
    "knowledge.experience.preparing": "文件扫描或索引准备阶段，暂时没有 LLM 流输出。",
    "knowledge.experience.waitingStream": "等待 JSON 流输出…",
    "knowledge.experience.recentRequests": "最近请求（最多 3 条）",
    "knowledge.experience.noHistory": "暂无已完成请求记录。",
    "knowledge.experience.currentFile": "当前文件：",
    "knowledge.experience.unknown": "未提供",
    "settings.modelExperience.embeddingNoMatch": "没有匹配的向量模型",
    "settings.modelExperience.curationUnavailable": "已选整理模型不可用，请重新选择",
    "settings.modelExperience.embeddingUnavailable": "已选向量模型不可用，请重新选择",
    "settings.modelExperience.chooseCuration": "选择整理模型",
    "settings.modelExperience.chooseEmbedding": "选择向量模型",
    "settings.modelExperience.noCuration": "没有可用的整理模型",
    "settings.modelExperience.noEmbedding": "没有可用的向量模型",
    "memory.workspace.read": "查看",
    "memory.workspace.purpose": "用途",
    "memory.workspace.actions": "操作",
    "memory.workspace.manage": "管理",
    "memory.workspace.settings": "设置",
    "memory.workspace.service": "模型与服务",
    "memory.workspace.browse": "会话与记忆",
    "memory.workspace.search": "语义查询",
    "memory.workspace.searchScope": "查询使用当前工作项目的召回范围，与会话浏览中选择的记忆项目独立。",
    "memory.workspace.filters": "筛选条件",
    "memory.workspace.all": "全部",
    "memory.workspace.bound": "已绑定",
    "memory.workspace.unbound": "未绑定",
    "memory.workspace.overrides": "有覆盖",
    "memory.workspace.noMatches": "没有匹配结果",
    "memory.workspace.selectedCount": "已选择 {count} 项",
    "memory.workspace.clearFilters": "清除筛选",
    "memory.workspace.dateHint": "按本地时区筛选最近活动；结束日期包含当天。",
    "memory.workspace.updatesAvailable": "会话有更新，刷新后查看",
    "memory.workspace.missingOutputs": "以下提取产物不在当前快照中，可刷新确认其状态：",
    "memory.workspace.missingSource": "来源回合不在当前快照中，可能已移除或不属于当前查询范围。",
    "memory.workspace.disabled": "当前作用域未启用记忆",
    "memory.workspace.not_ready": "记忆尚未就绪，仍可配置会话策略",
    "memory.workspace.ready": "记忆已就绪",
    "memory.workspace.selectSession": "选择会话查看原始对话、提炼结果与长期内容。",
    "memory.workspace.editScope": "编辑范围",
    "memory.workspace.basic": "启用与自动处理",
    "memory.workspace.globalPolicy": "全局处理规则",
    "memory.workspace.recallPolicy": "召回与提炼",
    "memory.workspace.textPolicy": "文本清洗",
    "memory.workspace.profilePolicy": "画像期限",
    "memory.workspace.rankingPolicy": "检索排序",
    "memory.workspace.retentionPolicy": "定期清洗",
    "memory.advanced.catalogLoading": "正在加载记忆项目…",
    "memory.advanced.catalogFailed": "记忆项目目录加载失败。",
    "memory.advanced.catalogRetry": "重新加载目录",
    "memory.workspace.scopeHint": "先选择范围。全局默认不会覆盖项目已有的独立设置。",
    "memory.workspace.recordInfo": "状态与记录信息",
    "memory.workspace.projectRules": "项目独立规则",
    "memory.workspace.effectiveFiles": "当前生效组合",
    "memory.workspace.otherFiles": "其他资源文件",
    "memory.workspace.index": "检索索引",
    "memory.workspace.cleanup": "清洗运行状态",
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
    "area.contextEngine": "代码语义索引",
    "area.knowledge": "知识库",
    "area.settings": "设置",
    "area.chat.description": "项目会话、消息流与模型输入区",
    "area.skills.description": "LuaSkills 技能、插件与运行时设置",
    "area.agentSkills.description": "标准 Agent Skills 技能目录",
    "area.agents.description": "智能体、专家画像与调度提示词",
    "area.memory.description": "VMM 用户、项目与会话记忆",
      "area.contextEngine.description": "ContextEngine 代码语义索引、模型路由与运行状态",
      "area.knowledge.description": "Markdown、Jieba 与 BM25 项目资料",
      "knowledge.name": "知识库名称",
      "knowledge.name.placeholder": "新知识库名称",
      "knowledge.create": "创建",
      "knowledge.filter.aria": "筛选知识库",
      "knowledge.filter.placeholder": "筛选知识库",
      "knowledge.refresh": "刷新",
      "knowledge.emptyLibraries": "尚无知识库。",
      "knowledge.librariesLoading": "正在读取知识库…",
      "knowledge.noMatchingLibraries": "没有匹配的知识库。",
      "knowledge.readFailed": "读取失败，请刷新重试。",
      "knowledge.libraryCounts": "{documents} 文档 · {chunks} 切片 · {status}",
      "knowledge.indexStatus.empty": "暂无内容",
      "knowledge.indexStatus.ready": "索引就绪",
      "knowledge.indexStatus.dirty": "索引待更新",
      "knowledge.indexStatus.rebuilding": "重建索引中",
      "knowledge.indexStatus.failed": "索引失败",
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
      "knowledge.composer.inherit": "使用项目默认",
      "knowledge.composer.loading": "正在更新知识库…",
      "knowledge.composer.readFailed": "暂时无法读取知识库选择。",
      "knowledge.composer.saveFailed": "选择未确认保存，请刷新状态后重试。",
      "knowledge.composer.reloadFailed": "选择已保存，暂时无法确认最新状态。",
      "knowledge.composer.retry": "刷新状态",
      "knowledge.composer.details": "查看详情",
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
    "appChrome.themeSaveFailed": "主题已在本次会话生效，但无法保存到当前客户端。",
    "appChrome.language.zh": "中文",
    "appChrome.scrollFeaturesLeft": "向左滚动主功能区",
    "appChrome.scrollFeaturesRight": "向右滚动主功能区",
    "appChrome.windowActionFailed": "窗口操作失败：",
    "appChrome.window.minimize": "最小化窗口",
    "appChrome.window.restore": "还原窗口",
    "appChrome.window.maximize": "最大化窗口",
    "appChrome.window.close": "关闭窗口",
    "app.shutdown.persisting": "正在保存会话记录并安全关闭…",
    "app.shutdown.persistFailed.title": "关闭前保存未完成",
    "app.shutdown.persistFailed.description": "无法确认最后一批会话记录已经耐久保存。取消后可保持窗口打开，再次关闭即可重试。",
    "app.shutdown.persistFailed.forceConfirm": "强制退出可能丢失最后一部分会话记录。是否仍要强制退出？",
    "app.shutdown.persistFailed.cancel": "取消并保持窗口",
    "app.shutdown.persistFailed.forceClose": "强制退出",
    "app.shutdown.persistFailed.forceClosing": "正在强制退出…",
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
    "conversation.submission.uploadFailed": "附件上传失败",
    "conversation.submission.uploadFailedAttachmentKept": "文件仍作为附件保留，本轮未发送文件内容。",
    "conversation.submission.uploadProgress": "媒体上传进度",
    "conversation.submission.sizeUnknown": "尚未获得文件总大小",
    "conversation.submission.cancelFailed": "未能取消上传，请重试。",
    "conversation.submission.cancelling": "正在取消上传",
    "conversation.loading.requesting": "正在请求会话数据",
    "conversation.loading.locating": "正在定位会话",
    "conversation.loading.index": "正在读取消息索引",
    "conversation.loading.reading": "正在读取消息内容",
    "conversation.loading.subagents": "正在恢复未关闭智能体索引",
    "conversation.loading.projecting": "正在整理显示数据",
    "conversation.loading.done": "会话载入完成",
    "conversation.loading.failed": "会话正文载入失败",
    "conversation.loading.retry": "重试载入",
    "conversation.unlinkedProject": "未关联项目",
    "conversation.history.loadMore": "加载更多历史",
    "conversation.history.loading": "正在加载历史",
    "conversation.history.invalidResponse": "历史分页响应不属于当前会话，已拒绝合并。",
    "conversation.history.invalidCursor": "历史分页没有推进到更早位置，已停止合并以避免重复加载。请重试并查看会话历史分页诊断日志。",
    "conversation.history.emptyPageWithMore": "本次没有加载到任何历史，但服务端仍声明存在更早记录。请重试并查看会话历史分页诊断日志。",
    "conversation.history.loadFailed": "加载更早历史失败",
    "conversation.action.copyFinal": "复制最终输出原文",
    "conversation.action.copiedFinal": "已复制最终输出原文",
    "conversation.action.fork": "分叉会话",
    "conversation.action.forkTitleSuffix": "（分叉）",
    "conversation.action.forkFailed": "分叉失败",
    "conversation.action.forkSucceeded": "已创建分叉会话",
    "conversation.action.forkInProgressTitle": "正在分叉会话",
    "conversation.action.forkInProgressMessage": "正在复制会话并打开新分叉，请稍候。",
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
    "conversation.systemError.pendingTitle": "结果与费用待确认",
    "conversation.systemError.pendingStatus": "待确认",
    "conversation.systemError.pendingNotice": "供应商已接纳此请求，但尚未确认最终结果与费用。系统不会自动重发，避免重复执行或重复计费；请使用请求标识在供应商请求历史中核对。",
    "conversation.systemError.resultConfirmation": "结果状态",
    "conversation.systemError.usageConfirmation": "费用状态",
    "conversation.systemError.unconfirmed": "待确认",
    "conversation.systemError.requestIdentifier": "请求标识",
    "conversation.systemError.requestIdentifierSource": "标识来源",
    "conversation.systemError.upstreamErrorPayload": "上游错误载荷",
    "conversation.systemError.identifierMissing": "供应商未提供",
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
    "conversation.providerRetry.retrying": "正在重试",
    "conversation.providerRetry.succeeded": "重试成功",
    "conversation.providerRetry.failed": "重试失败",
    "conversation.modelSwitch.title": "模型切换",
    "conversation.modelSwitch.changed": "模型已从 {previous} 切换为 {current}。",
    "conversation.goalNotice.saved": "目标已保存",
    "conversation.goalNotice.resumed": "目标已恢复，继续执行",
    "conversation.goalNotice.paused": "目标已暂停",
    "conversation.goalNotice.blocked": "目标受阻，已暂停自动续跑",
    "conversation.goalNotice.completed": "目标已完成",
    "conversation.goalNotice.cleared": "目标已移除",
    "conversation.goalNotice.budgetExhausted": "目标预算已用尽，已暂停",
    "conversation.goalNotice.usageLimited": "目标用量受限，已暂停",
    "conversation.goalNotice.usageUnknown": "目标用量暂不可核算，已暂停",
    "conversation.goalNotice.updated": "目标状态已更新",
    "composer.draft.retry": "重试加载草稿",
    "conversation.modelSwitch.help": "模型切换说明",
    "conversation.modelSwitch.warning": "在对话中途切换模型可能降低性能表现。背景信息可能会自动压缩。",
    "conversation.reasoningSwitch.title": "思考强度",
    "conversation.reasoningSwitch.nextProviderRequest": "思考强度已从 {previous} 调整为 {current}；当前正在进行的 Provider 请求保持冻结，新强度从下一次真实 Provider 请求开始生效。",
    "conversation.continue": "继续",
    "conversation.continuing": "继续中",
    "conversation.continuation.claimFailed": "继续失败，请查看会话状态后重试。",
    "conversation.continuation.blocked": "当前终止边界不可安全继续",
    "conversation.continuation.blocked.default": "当前开放回合未通过继续资格校验，无法继续。",
    "conversation.continuation.blocked.activeApi": "当前仍有物理 API 请求进行中，不能重复继续。",
    "conversation.continuation.blocked.missingApiStarted": "当前开放回合没有可关联的物理 API 起始事实，无法继续。",
    "conversation.continuation.blocked.duplicateApiTerminal": "当前物理 API 存在重复终态事实，无法继续。",
    "conversation.continuation.blocked.missingApiTerminal": "当前物理 API 尚无持久化终态事实，无法继续。",
    "conversation.continuation.blocked.apiTerminalOrder": "当前物理 API 的终态顺序无效，无法继续。",
    "conversation.continuation.blocked.terminalBoundarySuperseded": "该终态边界已被后续回合事实取代，请刷新会话状态。",
    "conversation.continuation.blocked.terminalPolicyForbidden": "该物理 API 的持久化终态明确禁止继续。",
    "conversation.continuation.blocked.routeChanged": "输入区选择的模型或连接与当前会话的下一请求配置不一致。",
    "conversation.continuation.blocked.sessionBusy": "会话当前仍有其他执行占用，无法继续。",
    "conversation.continuation.blocked.requestInvalid": "继续请求的客户端动作身份无效，服务已拒绝执行。",
    "conversation.continuation.blocked.conversationUnavailable": "目标会话不存在、标识无效，或不是可继续的已有会话。",
    "conversation.continuation.blocked.runtimeUnavailable": "目标会话的 V1 运行时所有者尚未就绪，无法继续。",
    "conversation.continuation.blocked.unknown": "继续请求被当前 V1 领域契约拒绝。",
    "conversation.continuation.timeout": "继续请求超时，已解除按钮锁定；请查看会话状态。",
    "conversation.continuation.stateSyncFailed": "服务已接收继续请求，但界面状态同步失败；请重试同一按钮以重新对账，不会创建第二次执行。",
    "conversation.continuation.ackMismatch": "继续响应与本次请求身份不一致，界面已拒绝该响应并重新读取权威状态。",
    "conversation.metadata.type": "类型",
    "conversation.metadata.status": "状态",
    "conversation.metadata.tool": "工具",
    "conversation.metadata.identifier": "标识",
    "conversation.metadata.error": "错误",
    "conversation.metadata.result": "结果",
    "conversation.metadata.returned": "已返回",
    "conversation.toolRequest.receiving": "正在接收数据",
    "conversation.toolRequest.argumentsAssembled": "参数已接收完成",
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
    "conversation.process.working": "正在工作",
    "conversation.process.worked": "已工作",
    "conversation.process.stoppedAfter": "你在 {duration} 后停止了",
    "conversation.process.stopped": "你已停止本轮工作",
    "conversation.process.duration": "持续 {duration}",
    "conversation.process.providerUsageBadge": "当前会话 Provider API {calls} 次 · {inFlight} 进行中 · Token {tokens}",
    "conversation.process.providerUsageSession": "当前会话：{calls} 次，Token {tokens}，未知 Usage {unknown} 次",
    "conversation.process.providerUsageUnknown": "未知",
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
    "composer.capability.loading": "正在加载会话能力…",
    "composer.capability.loadFailed": "会话能力加载失败",
    "composer.capability.retry": "重新加载",
    "composer.capability.unavailableEnabled": "已保存的开启状态无法生效。请选择可用目标，或关闭此能力。",
    "composer.capability.disableUnavailable": "关闭并保留选择",
    "composer.capability.removeUnavailable": "移除选择",
    "composer.capability.unavailableDetails": "查看已存模型与账号",
    "composer.capability.operationFailed": "设置未保存，请重试。",
    "composer.capability.errorDetails": "查看错误详情",
    "composer.capability.rebuild": "重建为默认配置",
    "composer.capability.rebuilding": "正在重建…",
    "composer.capability.rebuildHint": "可重新加载，或将当前会话的扩展能力设置重建为全局默认配置。重建会清除能力自定义选择，不影响对话记录、目标及其他配置。",
    "composer.capability.tooltipEmpty": "暂无已启用模块",
    "composer.capability.dialogTitle": "当前会话能力设置",
    "composer.capability.dialogDescription": "选择本会话的能力和执行目标。点击保存后，从下一次对话开始生效。",
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
    "composer.contextWindow.title": "压缩窗口",
    "composer.contextWindow.default": "默认（跟随模型）",
    "composer.contextWindow.invalid": "当前模型不可用",
    "composer.contextWindow.locked": "已锁定当前会话的压缩窗口。",
    "composer.contextWindow.defaultRestored": "已恢复跟随模型的默认压缩窗口。",
    "composer.contextWindow.previewUnavailable": "当前预览会话不能保存压缩窗口。",
    "composer.contextWindow.saveFailed": "压缩窗口未保存，请重试。",
    "composer.modelPicker.filter": "输入模型名称或 ID 过滤",
    "composer.modelPicker.noMatches": "当前过滤条件下没有匹配模型。",
    "composer.attachment.add": "添加附件",
    "composer.attachment.extension": "扩展",
    "composer.attachment.menu": "附件",
    "composer.attachment.empty": "当前无可用附件类型",
    "composer.attachment.remove": "移除附件",
    "composer.attachment.processing": "正在确认上传资源",
    "composer.attachment.preparationBlocked": "仍有 {count} 个附件正在准备，完成后才能发送。",
    "composer.attachment.metadataOnly": "此附件不会直接交给模型理解，仅会作为附件清单告知模型。",
    "composer.attachment.details": "查看附件详情",
    "composer.attachment.failed": "附件处理失败",
    "composer.attachment.retry": "重试上传",
    "composer.attachment.unsupported": "无法暂存这些附件。",
    "composer.attachment.maxLimit": "最多只能添加 {count} 个附件。",
    "composer.attachment.partialMaxLimit": "已达到 {count} 个附件上限，部分文件未添加。",
    "composer.attachment.kind.image": "图片",
    "composer.attachment.kind.audio": "音频",
    "composer.attachment.kind.video": "视频",
    "composer.attachment.kind.pdf": "PDF",
    "composer.attachment.kind.file": "文件",
    "composer.thinkingProcess": "思考弹窗",
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
  "composer.goal.status.blocked": "受阻的目标（已暂停）",
  "composer.goal.duration": "工作时长",
  "composer.goal.usage": "目标用量",
  "composer.goal.requests": "总请求次数",
  "composer.goal.tools": "工具调用次数",
  "composer.goal.cacheHit": "缓存命中率",
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
    "composer.error.draftLoadFailed": "会话草稿加载失败，发送已暂停。",
    "composer.error.draftPersistFailed": "会话草稿保存失败，请重试。",
    "composer.error.draftChangedBeforeSubmission": "草稿已变化，本次未发送，内容已保留。",
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
    "composer.accountCard.commandCodeMonthlyCredits": "每月额度",
    "composer.accountCard.commandCodePurchasedCredits": "购买额度",
    "composer.accountCard.commandCodeFreeCredits": "赠送额度",
    "composer.accountCard.commandCodeFiveHourRequests": "5 小时请求",
    "composer.accountCard.commandCodeWeeklyRequests": "每周请求",
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
    "conversationActions.previewCannotContinue": "浏览器预览模式下无法继续真实会话。",
    "conversationActions.modelSwitched": "已选择 {model}（{provider}）；该选择从下一次真实 Provider 请求开始生效。",
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
    "projectSettings.knowledgeBasesSavedRefreshFailed": "授权已保存，但刷新失败。请重新读取当前状态。",
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
    "projectSettings.codeContext.title": "代码语义索引",
    "projectSettings.codeContext.enabled": "已启用原生语义索引",
    "projectSettings.codeContext.disabled": "当前未启用",
    "projectSettings.codeContext.generating": "索引任务进行中，状态请查看下方进度。",
    "projectSettings.vmmDescription": "覆盖当前项目的记忆开关、用户/项目映射与召回注入参数。",
    "projectSettings.vmmNativeDescription": "覆盖当前项目的记忆开关、原生项目绑定与召回注入参数。",
    "projectSettings.vmmLoading": "正在加载项目 VMM 设置...",
    "projectSettings.enableProjectOverride": "启用项目覆盖",
    "projectSettings.overrideEnabledHint": "保存后当前项目使用下方设置。",
    "projectSettings.overrideDisabledHint": "关闭此开关并保存后，使用全局设置。",
    "projectSettings.vmmTransportHint": "项目级设置继承全局 gRPC 连接与 API Key；连接参数请在 VMM 记忆的全局配置中调整。",
    "projectSettings.vmmNativeHostHint": "原生记忆使用本机数据目录和已配置的 AI 能力。",
    "projectSettings.vmmNativeEnabled": "启用原生记忆",
    "projectSettings.vmmAdvanced": "高级注入参数",
    "projectSettings.saveVmm": "保存 VMM 设置",
    "projectSettings.vmmNativeBinding": "原生记忆项目绑定",
    "projectSettings.vmmNativeOwner": "写入归属项目",
    "projectSettings.vmmNativeLinked": "额外召回项目（仅查询）",
    "projectSettings.vmmNativeSaveBinding": "保存原生绑定",
    "projectSettings.vmmCreateBinding": "创建新的记忆项目绑定",
    "projectSettings.vmmCreateAndBind": "创建并绑定",
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
    "workspaceAudit.connecting": "正在连接当前终端…",
    "workspaceAudit.terminalFailure": "终端异常：{error}",
    "workspaceAudit.exited": "终端已退出",
    "workspaceAudit.exitedWithCode": "终端已退出（退出码 {code}）",
    "workspaceAudit.stopped": "终端已停止",
    "workspaceAudit.failed": "终端启动或运行失败",
    "workspaceAudit.copyTruncated": "已复制预览内容；文件过长，内容已截断。",
    "workspaceAudit.retry": "重试",
    "workspaceAudit.viewportLoading": "正在加载终端组件…",
    "workspaceAudit.aiReadOnly": "AI 控制 · 只读显示",
    "workspaceAudit.restoreAi": "还原 AI 终端窗口",
    "workspaceAudit.maximizeAi": "最大化 AI 终端窗口",
    "workspaceAudit.minimizeAi": "收起 AI 终端",
    "workspaceAudit.startupSteps": "已完成 {completed}/{total} 个步骤",
    "workspaceAudit.terminalLoading": "正在加载项目终端…",
    "workspaceAudit.emptyName": "终端名称不能为空。",
    "workspaceAudit.terminal": "项目终端",
    "workspaceAudit.expandList": "展开项目终端列表",
    "workspaceAudit.collapseList": "折叠项目终端列表",
    "workspaceAudit.restore": "还原项目终端窗口",
    "workspaceAudit.maximize": "最大化项目终端窗口",
    "workspaceAudit.minimize": "收起项目终端",
    "workspaceAudit.numberedList": "项目终端编号列表",
    "workspaceAudit.switchTerminal": "切换到终端 {index}：{name}",
    "workspaceAudit.chooseShell": "选择新终端 Shell",
    "workspaceAudit.create": "新建项目终端",
    "workspaceAudit.list": "项目终端列表",
    "workspaceAudit.name": "终端名称",
    "workspaceAudit.saveName": "保存 {name} 名称",
    "workspaceAudit.editName": "编辑 {name} 名称",
    "workspaceAudit.stop": "停止 {name}",
    "workspaceAudit.close": "关闭 {name}",
    "workspaceAudit.stopTitle": "停止终端",
    "workspaceAudit.closeTitle": "关闭终端",
    "workspaceAudit.empty": "选择 Shell 后点击 + 新建项目终端",
    "workspaceAudit.closeConfirm": "关闭运行中的终端？",
    "workspaceAudit.stopConfirm": "停止当前终端？",
    "workspaceAudit.closeDescription": "“{name}”仍在运行。确认后将停止完整进程树并移除该终端。",
    "workspaceAudit.stopDescription": "确认后将停止“{name}”的完整进程树，但保留终端输出标签。",
    "workspaceAudit.cancel": "取消",
    "workspaceAudit.processing": "处理中…",
    "workspaceAudit.confirm": "确认",
    "workspaceAudit.openDirectory": "打开目录：{path}",
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
    "workspaceShell.memoryTitle": "记忆工作区",
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
    "thinkingDisplay.popupOnly": "开启",
    "thinkingDisplay.inlineOutput": "关闭",
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
    "ask.custom.adjustCodex": "其他回答",
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
    "conversation.subagent.state.closed": "已关闭",
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
    "conversation.subagent.detail.usage": "智能体用量",
    "conversation.subagent.detail.result": "返回内容",
    "conversation.subagent.detail.noResult": "没有返回内容。",
    "conversation.subagent.detail.target": "目标",
    "conversation.subagent.detail.queued": "是否排队",
    "conversation.subagent.detail.steering": "是否插队",
    "conversation.subagent.queue.steering": "待插队",
    "conversation.subagent.queue.followup": "排队中",
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
"toolPresentation.output.loadFailed": "工具结果加载失败",
"toolPresentation.output.retryLoad": "重新加载",
"toolPresentation.output.errorDetails": "错误详情",
    "toolPresentation.output.waiting": "等待工具返回输出...",
    "toolPresentation.output.empty": "无输出",
    "toolPresentation.payload.input": "输入",
    "toolPresentation.payload.output": "输出",
    "toolPresentation.modelExtension.parameters": "获取模型扩展参数",
    "toolPresentation.raw.open": "显示原始工具调用输出",
    "toolPresentation.raw.title": "{tool} 原始调用",
    "toolPresentation.raw.legacyPartial": "当前仅有可见的部分调用信息。",
    "toolPresentation.execution.running": "执行中",
    "toolPresentation.exitCode": "退出代码",
    "toolPresentation.command.terminalTitle": "终端命令",
    "toolPresentation.command.shellTitle": "Shell 命令",
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
    "toolPresentation.native.todolist": "更新待办事项",
    "toolPresentation.native.getGoal": "读取目标",
    "toolPresentation.native.createGoal": "创建目标",
    "toolPresentation.native.updateGoal": "更新目标",
    "toolPresentation.native.knowledgeSearch": "搜索知识库",
    "toolPresentation.native.knowledgeBrowse": "浏览知识库",
    "toolPresentation.native.knowledgeRead": "读取知识库文档",
    "toolPresentation.native.skill": "查看技能",
    "toolPresentation.native.memorySearch": "搜索记忆",
    "toolPresentation.native.turnDetails": "读取回合详情",
    "toolPresentation.native.memoryWrite": "写入记忆",
    "toolPresentation.native.memoryDelete": "删除记忆",
    "toolPresentation.native.runtimeConfig": "运行配置",
    "toolPresentation.file.count": "{count} 个文件",
    "toolPresentation.file.multiChange": "多文件变更",
    "toolPresentation.file.empty": "无可展示内容",
    "toolPresentation.file.mixedLineEndings": "混合换行",
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
    "toolPresentation.file.edited": "已编辑",
    "toolPresentation.file.notEdited": "未修改",
    "toolPresentation.file.executionFailed": "执行失败",
    "toolPresentation.file.executionStopped": "已停止",
    "toolPresentation.file.executionCompleted": "执行完成",
    "toolPresentation.markdownOutput.running": "查询中",
    "toolPresentation.markdownOutput.failed": "查询失败",
    "toolPresentation.markdownOutput.stopped": "已停止",
    "toolPresentation.markdownOutput.completed": "已查询",
    "toolPresentation.fileList.title": "查询文件列表",
    "toolPresentation.fileRead.title": "读取文件",
    "toolPresentation.fileRead.rangeFrom": "从第 {start} 行开始",
    "toolPresentation.fileRead.range": "第 {start}-{end} 行",
    "toolPresentation.fileSearch.title": "搜索文件",
    "toolPresentation.fileStat.title": "读取文件信息",
    "toolPresentation.codekit.astTree.title": "列出 AST 树",
    "toolPresentation.codekit.astDetail.title": "查看 AST 详情",
    "toolPresentation.codekit.rg.title": "RG增强探索",
    "toolPresentation.codekit.nodeSource.title": "获取节点源码",
    "toolPresentation.codekit.markdownMenu.title": "查询 Markdown 目录",
    "markdown.wrap.enable": "开启自动换行",
    "markdown.wrap.disable": "关闭自动换行",
    "markdown.copyCode": "复制代码",
    "conversation.control.copyFailed": "复制失败",
    "conversation.control.actionFailed": "操作失败",
    "conversation.control.detailFailed": "详情读取失败",
    "conversation.control.retry": "重新读取",
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
    "memory.overview.title": "状态总览",
    "memory.overview.description": "查看记忆服务状态、完成首次配置并处理阻塞问题。",
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
    "vmm.experience.quickSetup.description": "使用推荐设置启用长期记忆。Vulcan Code 会检查所选后端、模型配置和数据安全状态后再启动。",
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
    "vmm.experience.defaultMemoryMode": "默认记忆模式",
    "vmm.experience.nativeBindingFixed": "绑定固定记忆空间",
    "vmm.experience.nativeBindingPerProject": "按项目独立创建",
    "vmm.experience.refresh": "刷新状态",
    "vmm.experience.start": "启动服务",
    "vmm.experience.restart": "重新启动",
    "vmm.experience.stop": "停止服务",
    "vmm.experience.reconfigure": "重新配置记忆后端",
    "vmm.experience.openAdvanced": "打开高级设置",
    "memory.identity.title": "用户与项目",
    "memory.identity.description": "管理长期记忆使用者与项目映射。内部编号仅用于服务路由，不会显示在页面中。",
    "memory.identity.nativeTitle": "记忆项目",
    "memory.identity.nativeDescription": "管理一级原生记忆项目、VulcanCode 项目归属、连带召回、迁移、清空和删除。",
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
    "memory.rules.description": "先选择编辑范围，再调整该范围的记忆规则。",
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
    "memory.rules.allProjects": "全局默认",
    "memory.rules.oneProject": "一个指定项目",
    "memory.rules.selectProject": "选择项目",
    "memory.rules.searchProject": "搜索项目…",
    "memory.rules.noProject": "没有可用项目",
    "memory.rules.applied": "设置已应用。",
    "memory.content.title": "记忆内容",
    "memory.content.retrieval": "检索说明",
    "memory.content.finalScore": "最终检索分值",
    "memory.content.creationSource": "记忆创建来源",
    "memory.content.totalEvidence": "全部情境：支持 / 反驳",
    "memory.content.matchedEvidence": "本次匹配：支持 / 反驳",
    "memory.content.contextAdjustment": "情境净调整",
    "memory.content.contextNotObserved": "本次未评估情境证据，不代表没有已存证据。",
    "memory.content.description": "使用自然语言搜索、查看和维护当前用户与项目的长期记忆。",
    "memory.content.nativeDescription": "浏览记忆项目中的会话与长期内容，或按当前工作项目的召回范围进行语义查询。",
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
    "memory.nativeRecords.title": "原生记忆来源",
    "memory.sessionList.title": "记忆会话",
    "memory.sessionList.description": "按项目筛选会话摘要，选择后查看提取记录和生成的记忆。勾选仅对当前页有效。",
    "memory.sessionList.back": "返回会话列表",
    "memory.sessionList.projectSources": "查看项目全部来源",
    "memory.sessionList.search": "会话标题或 ID",
    "memory.sessionList.status": "会话状态",
    "memory.sessionList.extraction": "提取状态",
    "memory.sessionList.allStates": "全部会话状态",
    "memory.sessionList.allExtraction": "全部提取状态",
    "memory.sessionList.from": "开始日期",
    "memory.sessionList.before": "结束日期",
    "memory.sessionList.sort": "排序方式",
    "memory.sessionList.newest": "最近活动优先",
    "memory.sessionList.oldest": "最早活动优先",
    "memory.sessionList.titleSort": "标题顺序",
    "memory.sessionList.apply": "应用筛选",
    "memory.sessionList.loading": "正在加载会话摘要…",
    "memory.sessionList.failed": "未能加载会话列表。请刷新重试；详细原因请查看操作提示。",
    "memory.sessionList.detailFailed": "未能加载来源详情。请刷新重试；详细原因请查看操作提示。",
    "memory.sessionList.invalidDates": "请选择有效的起止日期，截止日期不得早于起始日期。",
    "memory.sessionList.selectPage": "全选当前页",
    "memory.sessionList.selected": "已选 {count} 个会话",
    "memory.sessionList.mixed": "批量归档要求选中会话全部为活动，取消归档要求全部为已归档。",
    "memory.sessionList.select": "选择会话",
    "memory.sessionList.empty": "没有符合筛选条件的会话。",
    "memory.sessionList.total": "共 {total} 个会话 · 第 {page} 页",
    "memory.sessionList.previous": "上一页",
    "memory.sessionList.next": "下一页",
    "memory.sessionArchive.active": "活动",
    "memory.sessionArchive.archived": "已归档",
    "memory.sessionArchive.recycled": "已回收",
    "memory.sessionArchive.forgotten": "已遗忘",
    "memory.sessionArchive.unregistered": "尚未建立提取会话",
    "memory.sessionArchive.archive": "归档会话",
    "memory.sessionArchive.unarchive": "取消归档",
    "memory.sessionArchive.description": "仅改变会话的管理分类，不删除记忆、画像或向量，也不关闭召回与提炼。预览有效期为五分钟；期间记录有变更时，需要重新预览。",
    "memory.sessionArchive.confirm": "确认变更",
    "memory.removal.sessions": "预览回收会话",
    "memory.removal.turns": "预览回收所选回合",
    "memory.removal.selectSession": "选择回收会话",
    "memory.removal.selectTurn": "选择回收回合",
    "memory.removal.title": "确认移入记忆回收站",
    "memory.removal.description": "将以下原始记录及关联普通记忆、向量移入回收站，不复制、不重建向量。保留全部用户画像和项目画像，不删除 VulcanCode 聊天会话。",
    "memory.removal.sessionEffect": "回收整个记忆会话后暂停其记忆处理，可在回收站恢复。",
    "memory.removal.turnEffect": "仅回收所选回合及其普通记忆；不改变记忆会话的启用状态。",
    "memory.removal.vectors": "关联向量",
    "memory.removal.hotTurns": "影响热窗口回合",
    "memory.removal.hotWindow": "当前热窗口大小",
    "memory.removal.context": "关联情境条目",
    "memory.removal.projectProfiles": "关联项目画像（保留）",
    "memory.removal.userProfiles": "关联用户画像（保留）",
    "memory.removal.retention": "从确认执行时起保留",
    "memory.removal.days": "天；到期后可由清洗任务永久清除。",
    "memory.removal.expirySuffix": "；到期后可由清洗任务永久清除。",
    "memory.rules.invalidNumbers": "次数与数量须为 0–4294967295 的整数；留空表示继承。请修正标记的输入后保存。",
    "memory.rules.features": "功能与权限",
    "memory.rules.retentionProfiles": "保留与画像",
    "memory.advanced.invalidNumbers": "传输参数须为非负整数；留空表示继承。请修正标记的输入后保存。",
    "memory.projectProfile.durationHint": "期限 = 基础期限 + 刷新次数 × 延长量，不超过最长保留。基础期限至少 1 毫秒且不大于最长保留；延长量可为 0；上限为 3650 天。每个单位只影响相邻数值，持久级始终不过期。",
    "memory.removal.sources": "查看准确来源会话与回合",
    "memory.removal.confirm": "确认移入回收站",
    "memory.removal.failed": "回收未完成，请查看动作错误并重新预览。预览过期或数据变化后不能继续使用原确认。",
    "memory.sessionRestart.action": "重新启用记忆",
    "memory.sessionRestart.description": "仅重新开启这些已遗忘会话后续的记忆读取与写入。不会恢复已删除的记忆、画像或向量，也不会重新发送或提炼历史对话。",
    "memory.sessionRestart.confirm": "确认重新启用",
    "memory.sessionRestart.failed": "未能重新启用记忆，请重新预览。详细原因请查看操作提示。",
    "memory.sessionArchive.failed": "未能完成归档操作，请重新预览。详细原因请查看操作提示。",
    "memory.nativeRecords.description": "按原生记忆项目和宿主会话查看回合提取状态、各级记忆数量及每条记忆的准确来源。",
    "memory.nativeRecords.project": "记忆项目",
    "memory.nativeRecords.session": "宿主会话",
    "memory.nativeRecords.selectProject": "选择记忆项目",
    "memory.nativeRecords.selectSession": "选择宿主会话",
    "memory.nativeRecords.sessionOverview": "会话概览",
    "memory.nativeRecords.refresh": "刷新记录",
    "memory.nativeRecords.noProjects": "当前没有可查看的原生记忆项目。",
    "memory.nativeRecords.noSessions": "该记忆项目尚未产生会话记录。",
    "memory.nativeRecords.noProjectRecords": "该记忆项目尚未产生任何回合或记忆记录。",
    "memory.nativeRecords.sessions": "会话记录",
    "memory.nativeRecords.turns": "回合",
    "memory.nativeRecords.memories": "记忆",
    "memory.nativeRecords.profileNodes": "画像节点",
    "memory.nativeRecords.completed": "已提取",
    "memory.nativeRecords.pending": "待提取",
    "memory.nativeRecords.processedUnknown": "已处理，结果未知",
    "memory.nativeRecords.bypassed": "已跳过",
    "memory.nativeRecords.corrupted": "已损坏",
    "memory.nativeRecords.lastActivity": "最近活动",
    "memory.nativeRecords.projectSummary": "整个记忆项目的回合状态与记忆等级汇总",
    "memory.nativeRecords.turnRecords": "回合提取记录",
    "memory.nativeRecords.projectTurnRecords": "项目全部回合提取记录",
    "memory.nativeRecords.sessionMemories": "本会话生成或写入的记忆",
    "memory.nativeRecords.projectMemories": "项目全部记忆",
    "memory.nativeRecords.projectOnly": "仅归属项目，无来源会话",
    "memory.nativeRecords.noTurns": "该会话没有规范回合记录。",
    "memory.nativeRecords.noMemories": "该会话尚未生成或写入持久记忆。",
    "memory.nativeRecords.generated": "生成记忆",
    "memory.nativeRecords.generatedMemories": "本回合生成的记忆",
    "memory.nativeRecords.noGeneratedMemories": "本回合未生成持久记忆。",
    "memory.nativeRecords.generatedProfiles": "本回合生成的画像",
    "memory.nativeRecords.noGeneratedProfiles": "本回合未生成用户或项目画像节点。",
    "memory.nativeRecords.noProfileNodes": "所选范围内尚未生成用户或项目画像节点。",
    "memory.nativeRecords.noUserPreview": "该回合没有可显示的用户内容。",
    "memory.nativeRecords.retryCount": "失败重试次数",
    "memory.nativeRecords.sourceTurn": "来源回合",
    "memory.nativeRecords.priority": "优先级",
    "memory.nativeRecords.tags": "标签",
    "memory.nativeRecords.statusReason": "状态原因",
    "memory.nativeRecords.expiresAt": "过期时间",
    "memory.nativeRecords.turn": "回合",
    "memory.nativeRecords.timeline": "完整会话内容",
    "memory.nativeRecords.previousTurns": "前序回合",
    "memory.nativeRecords.nextTurns": "后续回合",
    "memory.nativeRecords.analysisDetails": "提取详情",
    "memory.nativeRecords.lastError": "最近提取错误",
    "memory.nativeRecords.statusPending": "待提取",
    "memory.nativeRecords.statusCompleted": "已提取",
    "memory.nativeRecords.statusProcessedUnknown": "已处理，提取结果未知",
    "memory.nativeRecords.statusBypassed": "分析失败后已跳过",
    "memory.nativeRecords.statusCorrupted": "载荷损坏，已隔离",
    "memory.nativeRecords.levelSession": "会话级",
    "memory.nativeRecords.levelPhase": "阶段级",
    "memory.nativeRecords.levelStable": "稳定级",
    "memory.nativeRecords.levelPersistent": "持久级",
    "memory.nativeRecords.stateActive": "活跃",
    "memory.nativeRecords.stateSuperseded": "已替代",
    "memory.nativeRecords.stateDeleted": "已删除",
    "memory.nativeRecords.stateExpired": "已过期",
    "memory.nativeRecords.stateRecycled": "回收站中",
    "memory.nativeRecords.sourceTurnExtracted": "回合提取",
    "memory.nativeRecords.sourceDirect": "主动写入",
    "memory.nativeRecords.sourceImport": "导入",
    "memory.nativeRecords.sourceSystemSeed": "系统种子",
    "memory.nativeRecords.sourceLegacy": "旧版迁入",
    "memory.nativeRecords.categoryGeneral": "通用事实",
    "memory.nativeRecords.categoryArchitecture": "架构决策",
    "memory.nativeRecords.categoryTechSpec": "技术规格/API",
    "memory.nativeRecords.categoryBusiness": "业务逻辑",
    "memory.nativeRecords.categoryRequirement": "需求/待办",
    "memory.nativeRecords.categoryProject": "项目上下文",
    "memory.nativeRecords.categoryBugDebt": "缺陷/技术债",
    "memory.nativeRecords.categorySecurity": "安全策略",
    "memory.nativeRecords.roleUser": "用户",
    "memory.nativeRecords.roleAssistant": "助手",
    "memory.nativeRecords.roleSystem": "系统",
    "memory.nativeRecords.roleTool": "工具",
    "memory.data.title": "记忆数据",
    "memory.data.description": "按会话和回合查看长期记忆来源，并通过影响预览安全归档、回收或恢复数据。",
    "memory.data.managementReady": "管理服务正常",
    "memory.data.managementUnavailable": "管理服务不可用",
    "memory.data.extractionRuntimeTitle": "记忆提炼等待中",
    "memory.data.extractionRuntimeWaiting": "后置提炼服务尚未就绪；已稳定落库的回合会在服务恢复后自动重试。",
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
    "memory.profile.nativeDescription": "查看固定本地用户画像，或选择一个原生记忆项目查看项目画像，并通过自然语言进行修正。",
    "memory.profile.target": "画像目标",
    "memory.profile.user": "用户画像",
    "memory.profile.project": "项目画像",
    "memory.profile.nativeUserName": "本地用户",
    "memory.profile.nativeUserDescription": "原生记忆只有一个由 VulcanCode 管理的固定用户，无需选择用户。",
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
    "memory.profile.invalidBundleMode": "画像服务未返回明确的内容模式，无法显示预览。请刷新并检查服务响应。",
    "memory.profile.previewDescription": "这是提供给记忆服务的完整画像文本，仅在排查或核对时需要查看。",
    "memory.profile.nativePreviewDescription": "这是原生记忆内核返回的画像文本。完整模式包含固定用户与选定项目的组合内容；拆分模式只显示当前选中的画像类别。",
    "memory.profile.priority": "优先级",
    "memory.profile.level": "层级",
    "memory.profile.stateSummary": "画像状态",
    "memory.profile.stateSummaryDescription": "原生模式显示全部持久画像节点；只有当前活动节点会参与画像渲染和提示词注入。",
    "memory.profile.stateActive": "活动",
    "memory.profile.statePending": "待评审",
    "memory.profile.stateScheduled": "待生效",
    "memory.profile.stateInactive": "已停用",
    "memory.profile.stateExpired": "已过期",
    "memory.profile.notInjected": "不参与注入",
    "memory.profile.source": "来源",
    "memory.profile.sourceTurns": "来源回合",
    "memory.profile.statusReason": "状态原因",
    "memory.profile.updatedAt": "更新时间",
    "memory.profile.storedState": "持久状态",
    "memory.profile.sourceTurnExtract": "回合提取",
    "memory.profile.sourceManualInstruction": "手工画像指令",
    "memory.profile.sourceSystemSeed": "系统种子",
    "memory.profile.sourceRetained": "用户删除后保留",
    "memory.advanced.title": "高级设置",
    "memory.advanced.description": "选择内置原生或外置 gRPC 后端，并查看各自专属配置与技术诊断。",
    "memory.advanced.deployment": "运行方式",
    "memory.advanced.native": "内置原生 VMM（推荐）",
    "memory.advanced.nativeDescription": "VMM Core 直接运行在 Vulcan Code 内并复用宿主 AI 与 VLDB。",
    "memory.advanced.external": "连接外部 VMM",
    "memory.advanced.externalDescription": "连接由你或团队独立运行的 VMM 服务。",
    "memory.advanced.connection": "外部服务连接",
    "memory.advanced.endpoint": "gRPC 地址",
    "memory.advanced.apiKey": "访问密钥",
    "memory.advanced.managementEndpoint": "管理 API 地址",
    "memory.advanced.managementToken": "管理 Token",
    "memory.advanced.testManagement": "保存并测试管理连接",
    "memory.advanced.managementTimeout": "管理请求超时（毫秒）",
    "memory.advanced.defaultBinding": "新增项目默认绑定规则",
    "memory.advanced.nativeFixedProject": "默认固定原生记忆项目",
    "memory.advanced.externalPrefix": "自动创建项目前缀",
    "memory.advanced.nativeStorage": "由 Vulcan Code 管理的原生存储",
    "memory.advanced.nativeStorageDescription": "关系数据与 LanceDB 向量分别存放在 .vulcan/code/vmm 下；项目级迁移和清空请在“记忆项目”页面执行。",
    "memory.advanced.runtime": "本地运行",
    "memory.advanced.autoStart": "随 Vulcan Code 自动启动",
    "memory.advanced.manualStart": "仅手动启动",
    "memory.advanced.transport": "连接高级参数",
    "memory.advanced.postactionReliability": "PostAction 可靠性",
    "memory.advanced.postactionTimeout": "后台分析超时（例如 180s）",
    "memory.advanced.failurePassThreshold": "连续失败 Pass 阈值",
    "memory.advanced.maxConnectionsPerHost": "每主机最大推理连接数",
    "memory.advanced.externalReliability": "由外部 VMM 管理",
    "memory.advanced.externalReliabilityDescription": "外部模式下，后台分析超时与连续失败 Pass 阈值必须在外部 VMM 配置或环境变量中设置；连接上限由外部 VMM 自身的有界传输控制。Vulcan Code 不会伪装成可远程修改这些参数。",
    "memory.advanced.diagnostics": "技术诊断",
    "memory.advanced.diagnosticRuntime": "运行状态",
    "memory.advanced.diagnosticBackend": "当前后端",
    "memory.advanced.diagnosticGrpc": "服务地址",
    "memory.advanced.diagnosticController": "存储控制器",
    "memory.advanced.diagnosticData": "数据目录",
    "memory.advanced.diagnosticLog": "运行日志",
    "memory.advanced.data": "记忆数据",
    "memory.advanced.save": "保存高级设置",
    "memory.advanced.backupRebuild": "备份并重建",
    "memory.resources.title": "原生资源与规则",
    "memory.resources.description": "按提示词、PII 与噪音管理规则组合和文件。浏览其他文件不会切换当前组合；PII 与噪音组合包含 common 通用规则。",
    "memory.resources.invalidResponse": "原生 VMM 资源接口返回了不匹配的结果。",
    "memory.policy.title": "原生默认策略 · 召回与提炼",
    "memory.textPolicy.title": "文本入库清洗",
    "memory.hostPolicy.title": "本项目的原生业务参数",
    "memory.hostPolicy.description": "按当前 VulcanCode 项目分别覆盖参数，不改变记忆绑定、连带读取范围或固定用户画像。各组独立保存；未覆盖的组实时继承全局，连接与注入开关单独管理。",
    "memory.hostPolicy.inherited": "继承当前全局设置；编辑并保存后仅本项目覆盖",
    "memory.hostPolicy.overridden": "使用本项目覆盖",
    "memory.hostPolicy.inherit": "恢复本组全局继承",
    "memory.hostPolicy.saved": "本项目参数已保存，下一个命令生效。",
    "memory.hostPolicy.connection": "为本项目单独设置",
    "memory.profilePolicy.title": "画像保留期限",
    "memory.profilePolicy.description": "分别设置固定用户画像和项目画像默认期限。只影响之后生成或修订的画像，不重写已有截止时间。",
    "memory.profilePolicy.user": "用户画像",
    "memory.profilePolicy.project": "项目画像默认值",
    "memory.profilePolicy.transient": "短期",
    "memory.profilePolicy.situational": "情境",
    "memory.profilePolicy.stable": "稳定",
    "memory.profilePolicy.baseMs": "基础期限",
    "memory.profilePolicy.stepMs": "每次刷新延长",
    "memory.profilePolicy.maxMs": "最长保留",
    "memory.profilePolicy.unit": "时间单位（适用于本行三个数值）",
    "memory.profilePolicy.hint": "期限 = 基础期限 + 刷新次数 × 延长量，不超过最长保留。基础期限至少 1 毫秒且不大于最长保留；延长量可为 0；期限上限为 3650 天。切换单位会按新单位解释本行数值。持久级始终不过期。",
    "memory.profilePolicy.save": "保存画像期限",
    "memory.profilePolicy.saved": "画像期限已保存，仅对之后生成或修订的画像生效。",
    "memory.textPolicy.description": "仅影响之后提交的用户问题、中间交互和最终回答，不重新清洗历史。媒体与思考内容始终清理；PII 和噪音仍按各自设置执行。",
    "memory.textPolicy.fold": "折叠长代码与机器日志",
    "memory.textPolicy.budget": "每个文本片段的目标 Token 预算",
    "memory.textPolicy.budgetHint": "256–131072；按近似 Token 估算保留头尾，默认 1200。它不是整个模型请求的硬上限，后续 L1 的目标回合与历史预算单独设置。",
    "memory.textPolicy.save": "保存文本清洗设置",
    "memory.textPolicy.saved": "文本清洗设置已保存，仅对新入库片段生效。",
    "memory.policy.description": "独立设置对话前召回与对话后提炼参数。保存到用户 VMM 配置目录，供新命令使用；已经开始的命令保持原策略。本区设置为全局默认，不修改项目绑定。",
    "memory.policy.precheckHistory": "对话前召回 · 历史轮数",
    "memory.policy.precheckBudget": "对话前召回 · 历史 Token 预算",
    "memory.policy.recallTopK": "每个查询召回条数",
    "memory.policy.reviewLimit": "合并后送审条数上限",
    "memory.policy.reviewScore": "候选送审评分下限",
    "memory.policy.queryLimit": "检索查询数量上限",
    "memory.policy.postactionHistory": "对话后提炼 · 已提炼历史轮数",
    "memory.policy.postactionBudget": "对话后提炼 · 当前轮次与历史 Token 预算",
    "memory.policy.extractedPriority": "新提炼记忆 · 默认优先级",
    "memory.policy.extractedWeight": "新提炼记忆 · 初始刷新权重",
    "memory.policy.extractedExpiry": "新提炼记忆 · 有效期",
    "memory.policy.extractedStability": "新提炼记忆 · 保留稳定性",
    "memory.policy.extractedLevel": "新提炼记忆 · 记忆等级",
    "memory.policy.working": "短期工作",
    "memory.policy.stable": "稳定保留",
    "memory.policy.archival": "归档保留",
    "memory.policy.expiryRange": "1 毫秒至 3650 天，必须能精确表示为整数毫秒。",
    "memory.policy.extractedHint": "这些默认值仅影响之后构建的提炼命令，不改写已有记忆，不改变画像规则或写入归属；有效期从原始回合发生时间起算，不是保存配置的时间。",
    "memory.policy.days": "天",
    "memory.policy.hours": "小时",
    "memory.policy.minutes": "分钟",
    "memory.policy.seconds": "秒",
    "memory.policy.milliseconds": "毫秒",
    "memory.policy.durationUnit": "有效期单位",
    "memory.policy.userSource": "当前来源：用户保存的全局策略",
    "memory.policy.defaultSource": "当前来源：内置默认值（尚未写入用户配置）",
    "memory.policy.scoreHint": "送审评分使用各查询内的审核排名投影，不是原始向量或全文分数。Token 预算约束业务上下文，不代表整个模型请求的硬上限。",
    "memory.policy.invalidValues": "请填写范围内的有效数值；除评分外，其他参数必须为整数。",
    "memory.policy.invalidResponse": "原生策略接口未返回有效结果，当前草稿尚未保存。",
    "memory.policy.saved": "策略已保存，新命令将使用此设置。",
    "memory.policy.save": "保存召回与提炼策略",
    "memory.policy.resetDraft": "填入内置默认值（保存后生效）",
    "memory.ranking.title": "原生默认策略 · 检索排序",
    "memory.cleanup.title": "原生默认策略 · 定期清洗",
    "memory.projectRetention.title": "项目数据保留规则",
    "memory.projectProfile.title": "项目画像规则",
    "memory.projectProfile.description": "按记忆项目设置画像的初始期限、刷新延长量和上限。新规则用于新的画像处理命令，不重写已有画像；个人画像始终使用全局规则。",
    "memory.projectProfile.inherited": "当前继承全局项目画像规则",
    "memory.projectProfile.saved": "项目画像规则已保存，将用于新的画像处理命令。",
    "memory.projectRetention.description": "按记忆项目设置热回合窗口、闲置与回收期限及保护规则。保存后用于下一轮清洗；已有回收批次期限不变。扫描开关、周期与个人画像仍使用全局设置。",
    "memory.projectRetention.select": "选择记忆项目",
    "memory.projectRetention.reload": "重新读取规则",
    "memory.projectRetention.inherited": "当前继承全局数据保留规则",
    "memory.projectRetention.overridden": "当前使用此记忆项目的独立规则",
    "memory.projectRetention.saved": "项目保留规则已保存，将用于下一轮清洗。",
    "memory.projectRetention.save": "保存项目覆盖",
    "memory.projectRetention.inherit": "恢复全局继承",
    "memory.projectRetention.fillGlobal": "填入当前全局值（尚未保存）",
    "memory.cleanup.description": "调整自动清洗周期和保留规则。关闭自动清洗后仍可手动运行；新规则用于下一轮，已有回收批次保留原到期时间。",
    "memory.cleanup.enabled": "启用定期清洗与画像过期扫描",
    "memory.cleanup.scan": "完整清洗周期（分钟）",
    "memory.cleanup.profileScan": "画像过期扫描周期（秒）",
    "memory.cleanup.hotWindow": "每个会话保留的最新回合数",
    "memory.cleanup.idle": "会话闲置多久后允许清洗",
    "memory.cleanup.trash": "新回收批次的保留时长",
    "memory.cleanup.protection": "高级 · 共享记忆保护",
    "memory.cleanup.protectionHint": "启用保护时，达到优先级或记忆等级任一阈值的共享终态记忆会被保留。仅控制自动回收，不改变已有记忆的等级。",
    "memory.cleanup.protectShared": "保护重要的共享终态记忆",
    "memory.cleanup.priority": "受保护的最低优先级（0–100）",
    "memory.cleanup.level": "受保护的最低记忆等级",
    "memory.cleanup.session": "会话级",
    "memory.cleanup.phase": "阶段级",
    "memory.cleanup.stable": "稳定级",
    "memory.cleanup.persistent": "持久级",
    "memory.cleanup.unit": "单位",
    "memory.cleanup.days": "天",
    "memory.cleanup.hours": "小时",
    "memory.cleanup.minutes": "分钟",
    "memory.cleanup.seconds": "秒",
    "memory.cleanup.milliseconds": "毫秒",
    "memory.cleanup.invalid": "请输入范围内的完整整数；周期、保留时长与回合数必须大于零。",
    "memory.cleanup.save": "保存清洗设置",
    "memory.cleanup.saved": "清洗设置已保存，工作器将读取新配置。",
    "memory.ranking.description": "调整新检索的候选与排序。保存不会重新生成向量，也不会重新提炼历史聊天；读取时衰减不会删除记忆。",
    "memory.ranking.hybrid": "向量与全文融合",
    "memory.ranking.mmr": "多样性选择（MMR）",
    "memory.ranking.decay": "读取时衰减",
    "memory.ranking.advanced": "高级排序参数",
    "memory.ranking.lexical": "混合检索候选窗口下限",
    "memory.ranking.rrf": "排名融合常数（RRF）",
    "memory.ranking.replace": "替代候选审核评分下限",
    "memory.ranking.dedupe": "硬去重余弦阈值（0 关闭）",
    "memory.ranking.dedupePool": "硬去重候选窗口",
    "memory.ranking.replacePool": "替代审核相似记忆数量",
    "memory.ranking.rerank": "重排候选数量（0 关闭）",
    "memory.ranking.lambda": "MMR 相关性权重",
    "memory.ranking.shape": "Weibull 分布形状",
    "memory.ranking.scale": "Weibull 时间尺度（小时）",
    "memory.ranking.floor": "衰减倍率下限",
    "memory.ranking.reinforce": "强化权重",
    "memory.ranking.crossSession": "跨会话采纳加成",
    "memory.ranking.scoreHint": "混合候选窗口由向量与全文共用。替代候选按审核后的评分筛选；硬去重单独使用余弦相似度，不能与全文 BM25 分值混用。关闭阶段会保留其参数，供再次开启时使用。",
    "memory.ranking.save": "保存检索排序策略",
    "memory.ranking.saved": "排序策略已保存，新命令将使用此设置。",
    "memory.ranking.invalidValues": "请填写有效范围内的数值。候选数量必须为整数，RRF 必须是无前导零的正十进制整数。",
    "memory.resources.loading": "正在加载原生资源…",
    "memory.resources.refresh": "刷新资源",
    "memory.resources.promptBundle": "记忆提示词语言",
    "memory.resources.piiLocale": "PII 国家/区域",
    "memory.resources.noiseLocale": "噪音国家/区域",
    "memory.resources.noiseEnabled": "启用噪音过滤",
    "memory.resources.noiseSemanticEnabled": "启用语义噪音匹配",
    "memory.resources.noiseThreshold": "语义噪音默认阈值",
    "memory.resources.saveComposition": "保存并立即应用",
    "memory.resources.fileEditor": "资源文件编辑器",
    "memory.resources.fileEditorDescription": "每份文件都可单独查看和覆盖；PII 与噪音规则按 common 加所选区域组合。",
    "memory.resources.file": "资源文件",
    "memory.resources.userOverride": "用户覆盖",
    "memory.resources.builtIn": "内置原文",
    "memory.resources.highRisk": "高风险提示词修改",
    "memory.resources.promptHowTo": "必须保留结构化输出约束、字段语义与阶段边界；错误修改会导致记忆遗漏、误写或结构化响应失败。",
    "memory.resources.beginPromptEdit": "了解风险并准备编辑",
    "memory.resources.builtInSource": "内置原文（只读）",
    "memory.resources.effectiveEditor": "生效内容 / 用户覆盖",
    "memory.resources.saveFile": "保存文件",
    "memory.resources.restoreBuiltIn": "恢复内置内容",
    "memory.resources.piiTester": "PII 规则测试器",
    "memory.resources.piiTesterDescription": "使用当前 common 与所选区域规则执行真实脱敏；规则中的 condition 公式由 Core 权威求值器执行，不调用 LLM。",
    "memory.resources.piiActiveBundleTest": "当前规则组合测试",
    "memory.resources.piiFormulaTester": "临时正则与 condition 公式测试",
    "memory.resources.piiFormulaTesterDescription": "不修改配置文件，直接使用 Core 的正则、捕获组、condition DSL 与 replacement 语义验证一条临时规则。",
    "memory.resources.piiSampleText": "测试文本",
    "memory.resources.piiPattern": "正则 pattern",
    "memory.resources.piiCondition": "条件 condition（可留空）",
    "memory.resources.piiReplacement": "替换 replacement",
    "memory.resources.piiCandidates": "正则候选：{count}",
    "memory.resources.piiAccepted": "公式通过",
    "memory.resources.piiRejected": "公式拒绝",
    "memory.resources.piiCaptures": "捕获组",
    "memory.resources.piiNoCandidates": "正则没有产生候选。",
    "memory.resources.noiseTester": "噪音规则测试器",
    "memory.resources.noiseTesterDescription": "这里只测试确定性正则；语义短语匹配由真实记忆录入链路执行。",
    "memory.resources.role": "消息角色",
    "memory.resources.runTest": "运行测试",
    "memory.resources.noMatch": "没有命中规则",
    "memory.resources.noiseMatched": "命中噪音类别",
    "memory.resources.confirmPurposeTitle": "确认你理解该规则文件的用途与风险",
    "memory.resources.confirmModificationTitle": "再次确认修改方法与风险",
    "memory.resources.confirmModificationDescription": "编辑后必须保持既有 JSON 契约与字段含义。确认后才会解锁编辑器和保存按钮；用户覆盖将写入 .vulcan/code/vmm/config。",
    "memory.resources.confirmPurpose": "我理解用途，继续",
    "memory.resources.unlockEditor": "我确认风险，解锁编辑",
    "memory.resources.promptPurpose.precheckL1": "PreCheck L1 在用户请求执行前做第一阶段记忆召回判定；修改必须保留它的结构化判定字段。",
    "memory.resources.promptPurpose.precheckL2": "PreCheck L2 在需要进一步分析时完成第二阶段召回决策；修改必须保留它的结构化判定字段。",
    "memory.resources.promptPurpose.postactionL1": "PostAction L1 在完成回合后提取候选记忆；修改会直接影响哪些信息进入长期记忆。",
    "memory.resources.promptPurpose.postactionL2": "PostAction L2 审核并整理候选记忆；修改会影响最终写入、层级、优先级与画像节点。",
    "memory.resources.promptPurpose.profileInstruction": "画像指令将用户自然语言调整转换为画像节点变更；修改必须保持操作与目标字段契约。",
    "memory.resources.promptPurpose.unknown": "这是 Core 结构化推理链路使用的提示词文件；修改前必须核对对应 Schema。",
    "memory.resources.overrideDirectory": "用户覆盖目录",
    "memory.resources.promptTab": "提示词设置",
    "memory.resources.piiTab": "PII 设置",
    "memory.resources.noiseTab": "噪音设置",
    "memory.resources.unlocked": "已解锁",
    "memory.resources.locked": "禁止编辑",
    "memory.resources.guardedEdit": "受保护的规则修改",
    "memory.resources.currentEffective": "所选文件内容",
    "memory.resources.composition": "当前规则组合",
    "memory.resources.compositionDescription": "只配置当前分页对应的语言、区域或策略；保存后立即校验并热更新。",
    "memory.resources.piiPurpose": "PII 规则在内容进入记忆处理前执行本地脱敏；正则、捕获组、condition 公式和 replacement 必须保持合法。",
    "memory.resources.noisePurpose": "噪音规则只判断当前 user prompt 是否应进入记忆分析；common 与所选国家/区域规则共同生效。",
    "memory.resources.saveConfirmTitle": "再次确认保存规则覆盖",
    "memory.resources.resetConfirmTitle": "确认恢复内置规则",
    "memory.resources.saveConfirmDescription": "保存将把当前内容写入 .vulcan/code/vmm/config 并立即热更新。错误提示词或规则可能导致记忆遗漏、误写或处理失败。",
    "memory.resources.resetConfirmDescription": "这会删除当前文件的用户覆盖，并立即恢复应用内置原文。",
    "memory.resources.saveConfirmAction": "确认保存并应用",
    "memory.resources.resetConfirmAction": "确认恢复内置内容",
    "memory.resources.userPromptOnly": "用户提示词（不接收 system、assistant 或 tool 内容）",
    "memory.maintenance.title": "清洗与回收",
    "memory.maintenance.description": "启用周期清洗后按当前设置运行；这里可查看策略、预览人工清洗影响并维护回收站。",
    "memory.maintenance.invalidResponse": "原生 VMM 清洗接口返回了不匹配的结果。",
    "memory.maintenance.readFailed": "读取失败，请刷新重试。",
    "memory.maintenance.runNow": "立即清洗",
    "memory.lexical.title": "全文检索索引",
    "memory.lexical.description": "使用 Jieba 分词搜索记忆摘要与详情。可从当前活跃记忆重新生成全文文档并重建索引，不调用模型、不重新生成向量。",
    "memory.lexical.documents": "全文源文档",
    "memory.lexical.rows": "全文索引行",
    "memory.lexical.rebuild": "重建全文索引",
    "memory.lexical.confirmTitle": "确认重建全文索引？",
    "memory.lexical.confirmDescription": "将从全部原生项目的 {count} 条活跃记忆重新生成全文索引，修复缺失或陈旧的派生文档。记忆正文、原向量、项目绑定与回收站内容均不改变。内容版本发生变化时将要求刷新后重新确认。",
    "memory.lexical.completed": "已重建 {count} 条全文索引，未生成向量。",
    "memory.lexical.countsNote": "计数用于对照，数量相等不代表分词内容一定完整；查询仍按项目权限和有效期过滤。",
    "memory.maintenance.previewCleanup": "预览清洗",
    "memory.maintenance.cleanupPreviewTitle": "确认清洗这些记录？",
    "memory.maintenance.cleanupPreviewDescription": "记忆与回合移入回收站，画像仅标记过期；已到期回收批次及其中内容将永久清除。预览有效期为 5 分钟，数据或规则改变后需要重新预览。范围为全部原生记忆项目，不受回收站筛选影响。",
    "memory.maintenance.cleanupImpact": "回收记忆 {memories} 条、回合 {turns} 个；过期画像 {profiles} 条；永久清除批次 {batches} 个。",
    "memory.maintenance.cleanupRecordIds": "查看受影响记录标识",
    "memory.maintenance.status": "定期清洗状态",
    "memory.maintenance.statusDescription": "清洗由宿主串行调度，关系数据与 LanceDB 向量通过同一耐久变更账本提交。",
    "memory.maintenance.worker": "完整清洗工作器",
    "memory.maintenance.profileExpiryWorker": "画像过期工作器",
    "memory.maintenance.running": "运行中",
    "memory.maintenance.enabled": "已启用",
    "memory.maintenance.disabled": "未启用",
    "memory.maintenance.interval": "运行周期",
    "memory.maintenance.profileExpiryInterval": "画像过期检查周期",
    "memory.maintenance.vectorTitle": "向量代次与重建",
    "memory.maintenance.vectorDescription": "嵌入模型或向量维度变化后，活动记忆和回收站内可恢复记忆必须显式重建到当前代次；预检不会修改数据。",
    "memory.maintenance.vectorConfigured": "当前嵌入路由",
    "memory.maintenance.configured": "已配置",
    "memory.maintenance.notConfigured": "不可用",
    "memory.maintenance.vectorDimension": "向量维度",
    "memory.maintenance.activeMemories": "活动记忆",
    "memory.maintenance.recycleBinMemories": "回收站记忆",
    "memory.maintenance.currentVectors": "活动记忆当前代次",
    "memory.maintenance.currentRecycleBinVectors": "回收站当前代次",
    "memory.maintenance.pendingAnalysisJobs": "待分析作业",
    "memory.maintenance.vectorRebuildRequired": "检测到旧代次向量。完成重建前，活动记忆读写或旧回收批次恢复会明确拒绝，避免混用不兼容向量。",
    "memory.maintenance.vectorRouteUnavailable": "当前无法解析嵌入路由，因此尚不能判断或重建向量代次。",
    "memory.maintenance.vectorCompatible": "所有活动记忆与回收站可恢复记忆均使用当前嵌入路由对应的向量代次。",
    "memory.maintenance.vectorRebuildCounts": "需要重建 {memories} 条记忆（活动 {active}，回收站 {recycled}），涉及 {spaces} 个旧向量代次。",
    "memory.maintenance.previewVectorRebuild": "预检向量重建",
    "memory.maintenance.confirmVectorRebuildTitle": "确认重建原生记忆向量？",
    "memory.maintenance.confirmVectorRebuildDescription": "将按当前嵌入路由重建 {memories} 条记忆（活动 {active}，回收站 {recycled}），覆盖 {projects} 个项目、{batches} 个有界批次，目标维度为 {dimension}。执行前会再次校验预检版本与路由。",
    "memory.maintenance.rebuildVectors": "确认重建",
    "memory.maintenance.pendingJobs": "待处理冷回合作业",
    "memory.maintenance.recycleBatches": "回收批次",
    "memory.maintenance.hotWindow": "每会话热回合保护数",
    "memory.maintenance.idleWindow": "闲置会话期限",
    "memory.maintenance.trashWindow": "回收站保留期限",
    "memory.maintenance.protectionFloor": "共享记忆保护阈值",
    "memory.maintenance.limits": "每轮清洗上限",
    "memory.maintenance.profileExpiryLimit": "到期画像节点",
    "memory.maintenance.memoryRecycleLimit": "终态记忆",
    "memory.maintenance.coldScanLimit": "冷回合会话扫描",
    "memory.maintenance.coldJobLimit": "冷回合作业执行",
    "memory.maintenance.idleSessionLimit": "闲置会话",
    "memory.maintenance.trashPurgeLimit": "到期回收批次",
    "memory.maintenance.vectorConsistency": "原生模式不运行独立向量 GC：活动数据与回收站向量由同一 VLDB 变更日志原子对账，永久清除批次时同步删除。",
    "memory.maintenance.lastRun": "最近完成",
    "memory.maintenance.profileExpiryLastRun": "最近画像过期检查",
    "memory.maintenance.profileExpiryLastResult": "本轮收敛 {profiles} 条到期画像。",
    "memory.maintenance.never": "尚未运行",
    "memory.maintenance.lastResult": "最近结果：收敛 {profiles} 条到期画像，回收 {memories} 条记忆、{turns} 个回合，入队 {enqueued} 个并完成 {completed} 个冷回合作业，永久清除 {purged} 个到期批次。",
    "memory.maintenance.recycleBin": "回收站",
    "memory.maintenance.recycleDescription": "回收期间保留原始关系记录和 LanceDB 向量；恢复不重新生成向量。若嵌入路由已变化，需先在上方完成包含回收站内容的向量重建；到期或手动永久清除后才删除。",
    "memory.maintenance.projectFilter": "记忆项目",
    "memory.maintenance.allProjects": "全部记忆项目",
    "memory.maintenance.emptyRecycleBin": "当前筛选范围没有回收批次。",
    "memory.maintenance.confirmPurgeTitle": "永久清除该回收批次？",
    "memory.maintenance.confirmRestoreTitle": "恢复该回收批次？",
    "memory.maintenance.confirmPurgeDescription": "该操作会永久删除批次内的关系记录和保留向量，无法撤销。",
    "memory.maintenance.confirmRestoreDescription": "恢复前会重新检查所有持久身份；如已产生冲突，操作将拒绝且不修改数据。",
    "memory.maintenance.purge": "永久清除",
    "memory.maintenance.restore": "恢复",
    "memory.maintenance.turns": "回合",
    "memory.maintenance.memories": "记忆",
    "memory.maintenance.profiles": "画像节点",
    "memory.maintenance.evidence": "情境证据",
    "memory.recycleRecords.inspect": "查看回收内容",
    "memory.recycleRecords.description": "仅查看本批次实际保留的记录，不恢复内容，不与活动记录合并。会话 ID 仅表示记录来源，并非批次原始选择。",
    "memory.recycleRecords.reload": "刷新明细",
    "memory.recycleRecords.total": "共 {count} 条",
    "memory.recycleRecords.failed": "无法加载回收明细，批次或版本可能已变化。请刷新重试，详细原因请查看操作提示。",
    "memory.recycleRecords.empty": "本批次没有此类记录。",
    "memory.recycleRecords.previous": "上一页",
    "memory.recycleRecords.next": "下一页",
    "memory.recycleRecords.session": "来源会话",
    "memory.recycleRecords.savedStatus": "回收时的提取状态",
    "memory.recycleRecords.sourceTurns": "来源回合",
    "memory.recycleRecords.userProfile": "用户画像",
    "memory.recycleRecords.projectProfile": "项目画像",
    "memory.recycleRecords.support": "支持",
    "memory.recycleRecords.rebuttal": "反驳",
    "memory.maintenance.expiresAt": "自动清除时间",
    "memory.maintenance.reasonTerminal": "终态记忆",
    "memory.maintenance.reasonColdTurn": "冷回合",
    "memory.maintenance.reasonIdle": "闲置会话",
    "memory.maintenance.reasonManualSession": "人工回收会话",
    "memory.maintenance.reasonManualTurn": "人工回收回合",
    "memory.maintenance.manualSources": "查看原始人工选择",
    "vmm.experience.deployment.native": "内置原生 VMM",
    "vmm.experience.deployment.disabled": "未启用",
    "vmm.experience.deployment.external": "外部服务",
    "vmm.experience.setup.notConfigured": "未配置",
    "vmm.experience.setup.checking": "正在检查",
    "vmm.experience.setup.ready": "已就绪",
    "vmm.experience.setup.incomplete": "需要完善",
    "vmm.experience.setup.blocked": "需要处理",
    "vmm.experience.lifecycle.stopped": "已停止",
    "vmm.experience.lifecycle.running": "运行中",
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
    "vmm.experience.prerequisite.transport.description": "选择内置原生记忆或外部服务，并完成对应配置。",
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
    "vmm.nativeProjects.title": "原生记忆项目",
    "vmm.nativeProjects.description": "管理一级记忆项目、稳定 ID 绑定和仅召回的连带项目。写入始终只进入归属项目。",
    "vmm.nativeProjects.refresh": "刷新目录",
    "vmm.nativeProjects.running": "正在执行原生记忆项目操作…",
    "vmm.nativeProjects.createTitle": "为未绑定项目创建记忆项目",
    "vmm.nativeProjects.createDescription": "新名称默认使用稳定项目标签；创建后立即按 ID 绑定。",
    "vmm.nativeProjects.selectVulcanProject": "选择未绑定的 VulcanCode 项目",
    "vmm.nativeProjects.projectName": "记忆项目名称",
    "vmm.nativeProjects.create": "创建并绑定",
    "vmm.nativeProjects.bindingsTitle": "项目绑定与连带查询",
    "vmm.nativeProjects.bindingsDescription": "每个 VulcanCode 项目只有一个写入归属，可额外选择多个仅召回项目。",
    "vmm.nativeProjects.unbound": "不绑定（关闭项目记忆）",
    "vmm.nativeProjects.saveBinding": "保存绑定",
    "vmm.nativeProjects.linkedRecall": "额外连带查询项目",
    "vmm.nativeProjects.catalogTitle": "记忆项目目录",
    "vmm.nativeProjects.catalogDescription": "名称可以修改但绑定 ID 不变；受保护的 default 项目不可删除。",
    "vmm.nativeProjects.rename": "修改名称",
    "vmm.nativeProjects.clear": "清空内容",
    "vmm.nativeProjects.delete": "删除项目",
    "vmm.nativeProjects.protected": "受保护",
    "vmm.nativeProjects.migrateTitle": "迁移项目记忆",
    "vmm.nativeProjects.migrateDescription": "先检测来源、目标、迁移后数量与会话冲突；确认后移动内容，旧项目保留为空。",
    "vmm.nativeProjects.source": "来源记忆项目",
    "vmm.nativeProjects.target": "目标记忆项目",
    "vmm.nativeProjects.previewMigration": "检测迁移",
    "vmm.nativeProjects.preview.migration": "确认迁移项目记忆",
    "vmm.nativeProjects.preview.clear": "确认清空项目内容",
    "vmm.nativeProjects.preview.delete": "确认删除记忆项目",
    "vmm.nativeProjects.previewCounts": "会话 {sessions}，记忆 {memories}，项目画像 {profiles}，向量节点 {vectors}。",
    "vmm.nativeProjects.previewMigrationCounts": "来源当前：会话 {sourceSessions}，记忆 {sourceMemories}，项目画像 {sourceProfiles}，向量节点 {sourceVectors}；目标当前：会话 {targetSessions}，记忆 {targetMemories}，项目画像 {targetProfiles}，向量节点 {targetVectors}；迁移后目标：会话 {afterSessions}，记忆 {afterMemories}，项目画像 {afterProfiles}，向量节点 {afterVectors}。",
    "vmm.nativeProjects.previewConflicts": "检测到 {count} 个会话名称冲突。确认后将按 -1、-2 依次改名并迁移。",
    "vmm.nativeProjects.previewRecords": "查看将迁移的 {count} 条主记录",
    "vmm.nativeProjects.recordKind.session": "会话",
    "vmm.nativeProjects.recordKind.turn": "回合",
    "vmm.nativeProjects.recordKind.memory": "记忆",
    "vmm.nativeProjects.recordKind.project_profile": "项目画像",
    "vmm.nativeProjects.recordSession": "来源会话：{session}",
    "vmm.nativeProjects.previewOwners": "以下 VulcanCode 项目当前绑定到此记忆项目，删除后会解除绑定并关闭其 VMM：{projects}",
    "vmm.nativeProjects.previewLinks": "以下 VulcanCode 项目将失去对此记忆项目的连带查询：{projects}",
    "vmm.nativeProjects.confirmRenameMigration": "⚠ 自动改名并迁移",
    "vmm.nativeProjects.confirm": "确认执行",
    "vmm.setup.title": "快速配置长期记忆",
    "vmm.setup.description": "选择内置原生记忆或外置 VMM，并明确绑定当前尚未关联的项目。",
    "vmm.setup.stepLabel": "第 {current} 步，共 {total} 步",
    "vmm.setup.previous": "上一步",
    "vmm.setup.next": "下一步",
    "vmm.setup.mode.title": "记忆服务在哪里运行",
    "vmm.setup.mode.description": "内置模式直接使用 Vulcan Code 的 AI 与存储能力；已有独立 VMM 服务时选择外部连接。",
    "vmm.setup.native.title": "内置原生记忆",
    "vmm.setup.native.description": "记忆内核直接运行在 Vulcan Code 内，不提供额外外部接口。",
    "vmm.setup.external.title": "连接外部服务",
    "vmm.setup.external.description": "连接已由你或团队运行的 VMM，不启动本机进程。",
    "vmm.setup.inference.title": "选择记忆处理模型",
    "vmm.setup.inference.description": "为记忆提取、整理、向量和重排选择模型与账号。下拉列表会自动过滤到适合对应任务的模型。",
    "vmm.setup.inference.required": "必须先配置并启用向量模型，以及 PreCheck 与 PostAction 的 L1/L2 基础模型和账号。全部路由有效后才会初始化记忆；画像与重排可稍后补充。",
    "vmm.setup.inference.openCredentials": "配置模型账号",
    "vmm.setup.binding.title": "项目记忆绑定",
    "vmm.setup.binding.description": "选择新项目的默认规则，并逐项确认当前未绑定项目的处理方式。绑定使用稳定 ID，后续改名不会失效。",
    "vmm.setup.binding.fixed": "默认绑定固定项目",
    "vmm.setup.binding.fixedDescription": "新增项目默认绑定同一个记忆项目；内置模式默认使用受保护的 default。",
    "vmm.setup.binding.separate": "按项目自动创建",
    "vmm.setup.binding.separateDescription": "新增项目以稳定项目标签创建独立记忆项目。",
    "vmm.setup.binding.defaultProject": "默认固定记忆项目",
    "vmm.setup.binding.prefix": "自动创建前缀（团队/空间）",
    "vmm.setup.binding.unboundProjects": "当前未绑定项目",
    "vmm.setup.binding.none": "当前没有未绑定项目。",
    "vmm.setup.binding.create": "创建并绑定项目",
    "vmm.setup.binding.existing": "绑定已有项目",
    "vmm.setup.binding.selectProject": "选择已有项目",
    "vmm.setup.scope.title": "选择启用范围",
    "vmm.setup.scope.description": "先确定记忆身份，再决定立即应用到哪里；以后可在“记忆规则”中调整。",
    "vmm.setup.scope.all_projects": "立即对所有项目启用",
    "vmm.setup.scope.current_project": "仅对当前项目启用",
    "vmm.setup.scope.currentProjectUnavailable": "当前没有可关联的工作区项目",
    "vmm.setup.scope.not_yet": "暂不启用，只完成配置",
    "vmm.setup.confirm.title": "确认配置",
    "vmm.setup.confirm.description": "确认后将保存后端、默认绑定规则和每个未绑定项目的明确决定。",
    "vmm.setup.user": "记忆用户名称",
    "vmm.setup.user.placeholder": "例如：本地用户",
    "vmm.setup.project": "当前项目名称",
    "vmm.setup.project.placeholder": "例如：vulcan-code",
    "vmm.setup.safety": "已有绑定不会按名称重建；项目名称调整后仍由稳定 ID 保持关联。",
    "vmm.setup.cancel": "取消",
    "vmm.setup.submit": "配置并启动",
    "vmm.setup.running": "正在检查配置并建立项目绑定…",
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
    "vmm.runtime.mode.external": "外部 VMM",
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
    "runtime.thinkingTokenCount": "（{count} tokens）",
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
    "settings.providerDescription.alibaba": "Alibaba Cloud Model Studio 与 Token Plan 服务。",
    "settings.providerDescription.openrouter": "聚合多个模型与上游节点的统一模型路由平台。",
    "settings.providerDescription.apikeyfun": "提供多种兼容协议的第三方模型中转服务。",
    "settings.providerDescription.claudeapi": "提供 Anthropic 兼容协议的第三方模型中转服务。",
    "settings.providerDescription.qiniu": "七牛云提供的模型推理与兼容协议服务。",
    "settings.providerDescription.fennoai": "提供多种兼容协议的第三方模型中转服务。",
    "settings.providerDescription.code0": "提供 OpenAI 与 Anthropic 兼容协议的第三方模型中转服务。",
    "settings.providerDescription.commandcode": "提供 OpenAI 与 Anthropic 兼容协议、动态模型目录和账号用量查询的第三方模型中转服务。",
    "settings.providerDescription.tavily": "面向 AI 应用的网页搜索与内容抓取服务。",
    "settings.providerDescription.openai": "OpenAI 官方模型与 Codex 账号服务。",
    "settings.providerDescription.anthropic": "Anthropic 官方 Claude 模型与 Claude Code 账号服务。",
    "settings.providerDescription.google": "Google 官方 Gemini API 与 Code Assist 服务。",
    "settings.providerDescription.agentspace": "AgentSpace OpenAI Responses/Codex 兼容模型与 GPT Image 2 图片生成服务。",
    "settings.providerDescription.tokenrhythm": "提供静态模型目录、OpenAI Chat 兼容推理与图片生成能力的第三方模型中转服务。",
    "settings.providerName.tokenrhythm": "基元律动",
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
    "settings.modelBasics": "基础设置",
    "settings.providerRetry": "请求重试",
    "settings.providerUsage": "用量与计费",
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
    "settings.visualLab": "视觉实验室",
    "settings.gitCommit": "Git 提交",
    "settings.aboutLicenses": "关于与许可证",
    "settings.archivedRestore": "归档管理",
    "settings.modelProviders.description": "查看全部已实现供应商，并管理自定义兼容供应商。",
    "settings.credentialManagement.description": "管理模型账号、调用协议、用量与默认凭据。",
    "settings.modelList.description": "按供应商查看已启用模型，并核对当前工作台模型。",
    "settings.modelBasics.description": "上下文、请求重试、用量与计费等模型运行基础设置。",
    "settings.providerRetry.description": "配置普通错误与 429 限流的重试次数、等待间隔和递增规则。",
    "settings.providerUsage.description": "查看原生出口侧用量、费用来源、覆盖率与账户额度快照。",
    "settings.aiVendors.description": "多个语义 AI 工具共用的能力供应商与路由目标。",
    "settings.aiWebSearch.description": "网页搜索能力的启用状态与可用目标。",
    "settings.aiWebExtract.description": "网页抓取能力的启用状态与可用目标。",
    "settings.aiUnderstandImage.description": "图片理解能力的启用状态与可用目标。",
    "settings.aiUnderstandVideo.description": "视频理解能力入口与接入状态。",
    "settings.aiUnderstandAudio.description": "音频理解能力入口与接入状态。",
    "settings.aiImageGeneration.description": "图片生成能力的启用状态与可用目标。",
    "settings.aiImageEdit.description": "图片编辑能力的可选原生模型、账号与约束说明。",
    "settings.aiVideoGeneration.description": "视频生成能力的启用状态与可用目标。",
    "settings.aiVideoEdit.description": "视频编辑能力的可选原生模型、账号与约束说明。",
    "settings.aiVideoExtend.description": "视频延长能力的可选原生模型、账号与时长约束。",
    "settings.aiSpeechSynthesis.description": "语音合成能力的启用状态与可用目标。",
    "settings.aiSpeechTranscribe.description": "语音转录能力的可选原生模型、账号与输入约束。",
    "settings.aiMusicGeneration.description": "音乐生成能力的启用状态与可用目标。",
    "settings.aiMusicCover.description": "音乐翻唱能力的启用状态与可用目标。",
    "settings.mcpServers.description": "MCP Runtime、服务连接、工具、提示词与资源状态。",
    "settings.runtimeDiagnostics.description": "启动链路、运行时模块与诊断日志。",
    "settings.visualLab.description": "统一视觉变量、普通组件与产品定制组件的固定验收入口。",
    "settings.diagnostics.logs": "诊断日志",
    "settings.diagnostics.startupStatus": "启动状态",
    "settings.diagnostics.waitingStatus": "等待状态更新",
    "settings.diagnostics.providerRequestTrace": "Provider 请求追踪",
    "settings.diagnostics.providerRequestTrace.completeRequests": "分组请求与原始返回日志",
    "settings.diagnostics.providerRequestTrace.openDirectory": "打开当前会话追踪目录",
    "settings.diagnostics.providerRequestTrace.clearDirectory": "清除当前会话追踪",
    "settings.diagnostics.providerRequestTrace.clearConfirm": "仅删除当前会话 temp/provider-api-trace 中的诊断文件。确定继续吗？",
    "settings.diagnostics.providerRequestTrace.clearSucceeded": "当前会话的 Provider 追踪已清除。",
    "settings.diagnostics.providerRequestTrace.clearEmpty": "当前会话没有可清除的 Provider 追踪。",
    "settings.diagnostics.providerRequestTrace.actionFailed": "追踪目录操作失败",
    "settings.diagnostics.providerRequestTrace.description": "开启后，后台会把同一逻辑 Provider 请求及其每次真实重试记录到同一组中，保存最终线上请求、响应元数据、解析前原始返回、chunk 或 WebSocket 帧边界以及失败终态。所属会话的 temp/provider-api-trace 目录固定轮转 4 个逻辑请求槽位；每次尝试最多保留 64 MiB 原始返回，失败和手动终止时已收到的部分内容也不会丢弃。文件可能包含提示词、工具参数、工具结果与 Provider 原文，仅应在诊断期间开启。",
    "settings.diagnostics.providerRequestTrace.disabled": "已关闭",
    "settings.diagnostics.providerRequestTrace.recordedData": "记录内容",
    "settings.diagnostics.providerRequestTrace.recordedDataHelp": "同一请求及其重试归为一组，记录实际发送的请求、响应元数据、解析前原始返回、流式分块或 WebSocket 帧边界，以及失败状态。",
    "settings.diagnostics.providerRequestTrace.retention": "保存范围",
    "settings.diagnostics.providerRequestTrace.retentionHelp": "每个会话轮转保留 4 组请求，每次尝试最多保存 64 MiB 原始返回。失败或手动停止时，保留已收到的内容。文件位于会话目录：",
    "settings.diagnostics.providerRequestTrace.privacy": "隐私说明",
    "settings.diagnostics.providerRequestTrace.privacyHelp": "文件可能包含提示词、工具参数、工具结果和 Provider 原文，仅在排查问题时开启。",
    "settings.diagnostics.providerRequestTrace.enabled": "诊断中",
    "settings.diagnostics.providerRequestTrace.saveFailed": "保存失败",
    "settings.diagnostics.providerRequestTrace.switch": "记录 Provider 请求与原始返回",
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
    "settings.customModels.enabled": "启用",
    "settings.customModels.protocolPreference": "协议偏好",
    "settings.customModels.protocolPreferenceHint": "多协议供应商可逐模型固定执行协议；Auto 按固定优先级解析。",
    "settings.customModels.protocolAuto": "自动",
    "settings.customModels.protocolExplicit": "显式",
    "settings.customModels.protocolSelect": "选择协议",
    "settings.customModels.saveProtocolPreference": "保存协议偏好",
    "settings.customModels.disabled": "关闭",
    "settings.credentialModels.manage": "管理此账号的模型",
    "settings.credentialModels.title": "账号模型",
    "settings.credentialModels.description": "只控制此账号能否到达模型；会话使用与内部使用在模型列表中独立控制。",
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
    "settings.customProviders.protocols": "启用协议",
    "settings.customProviders.protocolsHint": "至少选择一个协议；单协议时模型不显示逐模型协议选择，多协议时显示 Auto 与已启用协议。",
    "settings.customProviders.protocolCollectionRequired": "必须至少启用一个兼容协议。",
    "settings.customProviders.protocolCollectionDuplicate": "协议集合不能包含重复协议。",
    "settings.customProviders.routeDrafts": "逐协议路由覆盖",
    "settings.customProviders.routeDraftsHint": "每个启用协议可独立覆盖基础 URL、端点路径、鉴权、发现与固定请求头；空值继承共享设置。",
    "settings.customProviders.routeDraftFor": "{protocol} 路由",
    "settings.customProviders.routeBaseUrl": "路由基础 URL 覆盖",
    "settings.customProviders.routeBaseUrlPlaceholder": "留空继承共享基础 URL",
    "settings.customProviders.routeEndpointPath": "路由端点路径",
    "settings.customProviders.routeAuthentication": "路由鉴权请求头",
    "settings.customProviders.routeDiscoveryMode": "路由模型目录",
    "settings.customProviders.routeDiscoveryPath": "路由模型目录路径",
    "settings.customProviders.routeHeaders": "固定请求头（每行 key: value）",
    "settings.customProviders.routeInherit": "继承共享设置",
    "settings.customProviders.advancedOptions": "高级选项",
    "settings.customProviders.sessionHeader": "常规会话 Session Header",
    "settings.customProviders.sessionHeaderHint": "可选。仅用于常规会话，例如 x-opencode-session；如果与已有或宿主请求头冲突，将忽略该配置。",
    "settings.customProviders.codexCapabilities": "Codex Responses 能力",
    "settings.customProviders.codexRemoteCompaction": "启用远端上下文压缩",
    "settings.customProviders.codexRemoteCompactionHint": "调用 /responses/compact；仅在未产生语义输出或副作用且可安全重试时回退到本地压缩。",
    "settings.customProviders.codexWebsocket": "尝试 Responses WebSocket",
    "settings.customProviders.codexWebsocketHint": "将此供应商标记为未验证 WS 覆盖；失败时仅在安全边界内回退 HTTP/SSE。",
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
    "settings.customModels.effortAdaptive": "自动（adaptive）",
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
    "settings.customModels.toolSchemaProjection": "工具 Schema 契约",
    "settings.customModels.toolSchemaProjectionHint": "默认使用轻量处理。仅在供应商契约经过明确验证后选择原样；严格模式仅用于 Gemini 一类限制更强的工具声明接口。",
    "settings.customModels.toolSchemaProjectionLightweight": "轻量（默认）",
    "settings.customModels.toolSchemaProjectionPassthrough": "原样",
    "settings.customModels.toolSchemaProjectionStrict": "严格",
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
    "settings.customModels.manualDescription": "可随时手动添加真实模型 ID，不依赖 /models 发现结果。系统会先在本地公共库中寻找能力模板。",
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
    "settings.contextWindowPolicy.title": "上下文窗口策略",
    "settings.contextWindowPolicy.description": "按原始模型窗口计算：默认 95% 有效窗口、90% 自动压缩线；留空表示继承上级设置。",
    "settings.contextBasics.title": "上下文基础设置",
    "settings.mediaDelivery.title": "媒体投递阈值",
    "settings.mediaDelivery.description": "主会话与独立 AI 扩展分别配置；单位 KiB（1 KiB = 1024 字节）。每项阈值包含边界值，零表示禁用该分支的 Base64，不影响附件保存。",
    "settings.mediaDelivery.uploadRule": "有真实上传时，小于等于相应阈值可直接内联，否则上传；无上传时使用独立阈值。模型、协议和上传接口硬限制始终优先，接口超限不能改走 Base64。",
    "settings.mediaDelivery.historyRule": "这些阈值用于新的投递意图；切换模型不会把旧上传资源重新上传，历史原投递身份保持不变。",
    "settings.mediaDelivery.conversation": "主会话（含子智能体会话）",
    "settings.mediaDelivery.extension": "独立 AI 扩展",
    "settings.mediaDelivery.kind": "媒体类型",
    "settings.mediaDelivery.image": "图片",
    "settings.mediaDelivery.audio": "音频",
    "settings.mediaDelivery.video": "视频",
    "settings.mediaDelivery.file": "文件 / PDF",
    "settings.mediaDelivery.withUpload": "有上传时内联上限",
    "settings.mediaDelivery.withoutUpload": "无上传时内联上限",
    "settings.mediaDelivery.aggregate": "单次请求累计 Base64 编码预算",
    "settings.mediaDelivery.rawMaximum": "后端允许的单项原始阈值上限",
    "settings.mediaDelivery.totalMaximum": "后端允许的累计编码预算上限",
    "settings.mediaDelivery.failureRule": "超限时主会话保留附件并提供安全地址；独立扩展明确报告无法理解或处理文件。Base64 膨胀、协议封装及实际上下文限制会进一步收紧可发送大小。",
    "settings.mediaDelivery.invalidNumber": "请输入非负十进制 KiB 数值，不能留空或使用指数格式。",
    "settings.mediaDelivery.invalidLimit": "输入超过后端允许的上限，请检查字段及下方限制说明。",
    "settings.mediaDelivery.invalidFraction": "KiB 数值换算后必须是整数字节，最小可设置单位为 1 字节。",
    "settings.mediaDelivery.conflict": "后台媒体设置已变化，未覆盖新设置。请重新载入后再编辑。",
    "settings.mediaDelivery.save": "保存媒体设置",
    "settings.mediaDelivery.reset": "恢复后端默认值",
    "settings.mediaDelivery.reload": "重新载入并放弃本地编辑",
    "settings.contextBasics.description": "配置模型上下文窗口、自动压缩和回退缓冲策略，并集中管理请求重试与用量计费。",
    "settings.contextBasics.loading": "正在读取基础设置…",
    "settings.contextBasics.saveSystemDefault": "保存系统默认",
    "settings.contextCompactionNodes.title": "上下文压缩窗口节点",
    "settings.contextCompactionNodes.description": "固定五个节点；只能修改 Token 值，不能新增、删除、重命名或调整顺序。",
    "settings.contextCompactionNodes.warning": "修改节点值会影响所有引用该节点的会话。",
    "settings.contextCompactionNodes.node1": "节点 1",
    "settings.contextCompactionNodes.node2": "节点 2",
    "settings.contextCompactionNodes.node3": "节点 3",
    "settings.contextCompactionNodes.node4": "节点 4",
    "settings.contextCompactionNodes.node5": "节点 5",
    "settings.contextCompactionNodes.invalidPositive": "每个节点必须是正的安全整数。",
    "settings.contextCompactionNodes.invalidAscending": "五个节点的 Token 值必须从低到高严格递增。",
    "settings.contextCompactionNodes.save": "保存节点配置",
    "settings.contextWindowPolicy.effectivePercent": "有效窗口百分比",
    "settings.contextWindowPolicy.autoCompactPercent": "自动压缩百分比",
    "settings.contextWindowPolicy.autoCompactTokenLimit": "自动压缩 Token 上限",
    "settings.contextWindowPolicy.scope": "压缩计量范围",
    "settings.contextWindowPolicy.scopeInherit": "继承上级",
    "settings.contextWindowPolicy.scopeTotal": "总用量（Total）",
    "settings.contextWindowPolicy.scopeBodyAfterPrefix": "稳定前缀之后（BodyAfterPrefix）",
    "settings.contextWindowPolicy.fallbackBuffer": "回退缓冲 Token",
    "settings.contextWindowPolicy.clear": "清除本层策略",
    "settings.models.conversationUse": "会话使用",
    "settings.models.multimodalUnderstandingCandidate": "多模态理解候选",
    "settings.models.multimodalUnderstandingUnavailable": "需要图片、视频或音频输入能力，以及 JSON Object 或 JSON Schema 输出能力",
    "settings.providerTools.mediaBackendMode": "理解后端模式",
    "settings.providerTools.selectBackendMode": "请选择理解后端模式",
    "settings.providerTools.providerToolMode": "供应商专用模式",
    "settings.providerTools.modelReplacementMode": "模型代替模式",
    "settings.providerTools.reasoningEffort": "思考级别",
    "settings.providerTools.modelDefaultReasoning": "模型固定 / 默认",
    "settings.models.internalUse": "插件调用",
    "settings.models.loadingCatalog": "正在读取实时模型目录…",
    "settings.models.provider": "供应商",
    "settings.models.liveModels": "实时模型",
    "settings.models.noProviderSelected": "未选择供应商",
    "settings.models.catalogSummary": "{provider} · 总计 {count}",
    "settings.models.memoryOnly": "仅保存在内存；启动和手动同步后更新",
    "settings.models.emptyCatalog": "当前没有已同步的模型。请先在“模型供应商”中添加账号并同步。",
    "settings.models.availableCredentials": "{count} 个可用凭据",
    "settings.models.additionalConditions": "{protocol} · 附加条件",
    "settings.models.routeEffectiveProtocol": "后端生效协议",
    "settings.models.routeAvailableProtocols": "可用协议",
    "settings.models.routeBindingSource": "绑定来源",
    "settings.models.routeBindingRule": "规则 ID",
    "settings.models.routeManifestRevision": "清单修订",
    "settings.models.routeUnresolved": "所选账号当前没有后端解析出的可执行协议绑定；该模型在此账号下处于阻塞状态。",
    "settings.models.routeUnavailableReason": "后端阻塞原因",
    "settings.models.routeNoRule": "默认绑定",
    "settings.models.blockedRouteDiagnostics": "不可执行模型路由",
    "settings.models.codexBehavior": "Codex 行为",
    "settings.models.requiredRuntimePrimitive": "缺失运行时原语",
    "settings.models.collapseProviders": "收起为胶囊",
    "settings.models.expandProviders": "展开供应商分类",
    "settings.models.searchLabel": "搜索模型",
    "settings.models.searchPlaceholder": "搜索模型名称或 ID",
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
    "settings.inference.vmmReasoningNotice": "全部 VMM LLM 用途都只显示明确支持关闭思考的模型，并将思考档位固定为关闭。推理输出始终与 VMM JSON 正文分离。",
    "settings.inference.profileVmm": "VMM 配置档",
    "settings.inference.llm": "受限 LLM",
    "settings.inference.embedding": "向量",
    "settings.inference.rerank": "重排",
    "settings.inference.rerankEnabled": "启用 Rerank",
    "settings.inference.rerankDisabled": "重排未启用（可选）。启用后选择模型和账号。",
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
    "settings.inference.reasoningDisabled": "关闭",
    "settings.inference.reasoningAdaptive": "自适应",
    "settings.inference.reasoningMinimal": "最小",
    "settings.inference.reasoningXHigh": "超高",
    "settings.inference.reasoningUltra": "极限",
    "settings.inference.embeddingTask": "向量任务",
    "settings.inference.embeddingDimensions": "向量维度",
    "settings.inference.embeddingOutputKind": "向量输出类型",
    "settings.inference.embeddingEncoding": "向量编码",
    "settings.inference.rerankTopN": "默认返回数量",
    "settings.inference.rerankTruncation": "截断策略",
    "settings.inference.rerankReturnContent": "返回候选正文",
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
    "settings.providerTools.imageEdit": "图片编辑",
    "settings.providerTools.mediaAnalyze": "图片理解工具",
    "settings.providerTools.musicGenerate": "音乐生成",
    "settings.providerTools.musicCover": "音乐翻唱",
    "settings.providerTools.speechSynthesize": "语音合成",
    "settings.providerTools.speechTranscribe": "语音转写",
    "settings.providerTools.videoGenerate": "视频生成",
    "settings.providerTools.videoEdit": "视频编辑",
    "settings.providerTools.videoExtend": "视频续长",
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
    "settings.providerTools.systemDefaultHint": "选择可用模型与账号后才能启用。目标失效时默认关闭；新会话创建时采用此配置。",
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
    "gitCommitDialog.monitoringManagerDescription": "选择需要监听的仓库并调整显示顺序。在仓库列表中切换当前操作对象。",
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
    "settings.aboutLicenses.description": "应用版本、许可证入口与合规说明。",
    "settings.archivedRestore.description": "查看、恢复或永久删除已归档的会话。",
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
    "skills.label.package": "技能包",
    "skills.label.plugin": "插件",
    "skills.catalogSummary": "系统记录 {system}，用户记录 {user}，插件记录 {plugin}，有效技能 {effective}，目录问题 {issues}。",
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
    "workspaceToolPanel.category": "分类",
    "skills.navigation.builtins": "内置插件与管理",
    "skills.navigation.externalPlugins": "插件列表",
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
    "projectMenu.openTerminal": "在终端中打开",
    "composer.contextUsage.cacheHitRate": "平均缓存命中率",
    "composer.contextUsage.noData": "暂无数据",
    "composer.contextUsage.usedPercent": "上下文已使用 {percent}%",
    "composer.contextUsage.totalRequests": "总请求",
    "composer.contextUsage.averageFirstToken": "平均首 Token",
    "composer.contextUsage.averageSpeed": "平均速度",
    "composer.contextUsage.noTiming": "暂无配对的首 Token 计时与输出用量",
    "composer.contextUsage.totalTokens": "总 Tokens",
    "composer.capability.mediaBackendMode": "后端模式",
    "composer.capability.mediaFixedRoute": "固定路由",
    "composer.capability.mediaBackendNote": "此处的模型、账号和思考档位仅用于当前会话，不修改系统默认设置。",
    "composer.capability.mediaBackendMissing": "请先在模型设置中配置此媒体理解能力的可用后端模式。",
    "composer.attachment.previewUnavailable": "此文件类型暂不支持预览，仍可随消息发送。",
    "composer.attachment.stagingPending": "正在暂存附件，完成后即可发送。",
    "composer.capability.extensionHint": "可以选择多个模型，AI 会按需使用。",
    "composer.capability.extensionProfileMissing": "缺少已审核模型扩展说明，当前不可选择。",
    "composer.capability.extensionConstraints": "约束：{value}",
    "composer.capability.extensionLimitations": "限制：{value}",
    "composer.capability.mode.provider": "供应商专用模式",
    "composer.capability.mode.model": "模型代替模式",
    "composer.capability.reasoning": "思考 {effort}",
    "composer.capability.inheritGlobalReasoning": "继承系统推理档位",
    "settings.usageRefresh.title": "账号用量后台刷新",
    "settings.usageRefresh.description": "仅刷新声明支持用量查询且已具备必要凭据的账号；默认每 30 分钟刷新，缓存只保存在内存中。",
    "settings.usageRefresh.minute": "每 {count} 分钟",
    "settings.usageRefresh.minutes": "每 {count} 分钟",
    "settings.usageRefresh.hour": "每 {count} 小时",
    "settings.credentials.selectProvider": "选择供应商",
    "settings.credentials.categoryAria": "凭据分类",
    "settings.credentials.collapse": "收起为胶囊",
    "settings.credentials.expand": "展开凭据分类",
    "settings.credentials.connecting": "正在连接公共 AI 运行时",
    "settings.credentials.connectingDescription": "启动后会从各凭据实时获取模型，不读取本地模型缓存。",
    "settings.credentials.emptyProvider": "尚未添加该供应商的凭据",
    "settings.credentials.emptyProviderDescription": "使用上方“添加凭据”按钮，为当前供应商创建第一个账号。",
    "settings.credentials.emptyAll": "尚未添加模型账号",
    "settings.credentials.emptyAllDescription": "点击“添加凭据”，选择供应商、服务入口、凭据方式与协议。",
    "settings.credentials.refreshUsage": "刷新账号用量",
    "settings.credentials.syncModels": "同步支持模型",
    "settings.credentials.enable": "启用凭据",
    "settings.credentials.disable": "停用凭据",
    "settings.credentials.disabledStatus": "已停用",
    "settings.credentials.currentDefault": "当前默认账号",
    "settings.credentials.setDefault": "设为默认账号",
    "settings.credentials.edit": "编辑凭据",
    "settings.credentials.remove": "删除凭据",
    "settings.credentials.deleteDefaultMissingReplacement": "删除默认账号前必须选择替代默认账号。",
    "settings.credentials.deleteTitle": "删除模型账号",
    "settings.credentials.deleteDescription": "将删除“{name}”及其系统凭据，此操作不可撤销。",
    "settings.credentials.replacementDefault": "替代默认账号",
    "settings.credentials.cancel": "取消",
    "settings.credentials.deleting": "删除中…",
    "settings.credentials.confirmDelete": "确认删除",
    "settings.credentials.usageUnsupported": "该凭据不支持账号用量查询",
    "settings.credentials.usageNotQueried": "用量尚未查询",
    "settings.credentials.usageUnavailable": "账号用量暂时不可用",
    "settings.credentials.usageInvalid": "账号用量响应格式无效，请重新刷新",
    "settings.credentials.noQuotaWindows": "该供应商不提供套餐额度窗口",
    "settings.credentials.quotaWindow": "套餐额度窗口",
    "settings.credentials.remaining": "剩余 {value}",
    "settings.credentials.used": "已用 {value}",
    "settings.credentials.notIncluded": "当前套餐未包含",
    "settings.credentials.unlimited": "无限",
    "settings.credentials.unknown": "未知",
    "settings.credentials.connectionIncomplete": "请完整填写连接信息。",
    "settings.credentials.editToolTitle": "编辑工具服务凭据",
    "settings.credentials.editModelTitle": "编辑模型账号",
    "settings.credentials.addToolTitle": "添加工具服务凭据",
    "settings.credentials.addModelTitle": "添加模型账号",
    "settings.credentials.toolDescription": "该凭据只用于所选供应商的原生 AI 工具，不会创建虚假的对话模型。",
    "settings.credentials.modelDescription": "协议绑定在当前账号上，模型调用期间不会切换协议或账号。",
    "settings.credentials.provider": "供应商",
    "settings.credentials.serviceEntry": "服务入口",
    "settings.credentials.autoRegion": "保存时自动识别区域",
    "settings.credentials.credentialType": "凭据类型",
    "settings.credentials.displayName": "账号显示名",
    "settings.credentials.replaceToken": "替换 Token（留空则不变）",
    "settings.credentials.apiToken": "API Token",
    "settings.credentials.managementKey": "管理密钥（可选）",
    "settings.credentials.managementKeyPlaceholder": "留空保持现有管理密钥",
    "settings.credentials.managementKeyHint": "仅用于该账号的用量查询，不用于模型调用。",
    "settings.credentials.removeManagementKey": "移除现有管理密钥",
    "settings.credentials.oauthClientSecretOptional": "Google OAuth Client Secret（可选）",
    "settings.credentials.authorizationStatus": "授权状态",
    "settings.credentials.authorizationCode": "授权码",
    "settings.credentials.cancelAuthorization": "取消授权",
    "settings.credentials.waitBrowser": "等待浏览器授权",
    "settings.credentials.saveSyncing": "保存并同步…",
    "settings.credentials.reauthorizeAndSave": "重新授权并保存",
    "settings.credentials.openBrowserAuthorize": "打开浏览器授权",
    "settings.credentials.saveSync": "保存并同步",
    "settings.credentials.oauth.waitingAuthorization": "等待用户确认授权",
    "settings.credentials.oauth.waitingCallback": "等待浏览器回调",
    "settings.credentials.oauth.finalizing": "正在完成凭据保存",
    "settings.credentials.oauth.succeeded": "授权成功",
    "settings.credentials.oauth.failed": "授权失败",
    "settings.credentials.oauth.cancelled": "已取消",
    "settings.providers.managementTitle": "供应商管理",
    "settings.providers.managementDescription": "管理 Vulcan Code 已实现的内置供应商与复用现有协议适配器的自定义供应商。",
    "settings.providers.system": "系统供应商",
    "settings.providers.functional": "功能供应商",
    "settings.providers.custom": "自定义供应商",
    "settings.providers.catalog": "供应商清单",
    "settings.providers.isolatedCount": "已隔离 {count} 个自定义供应商",
    "settings.providers.isolatedDescription": "该供应商已停止参与运行时路由；其他供应商及其设置仍可正常使用和保存。下列独立 Manifest 源文件会在保存时保持原始字节不变；可修复源文件后重启 Service，或显式删除对应隔离项。",
    "settings.providers.removeIsolated": "删除隔离项",
    "settings.providers.diagnosticsOmitted": "其余诊断已按安全上限省略。",
    "settings.providers.removeIsolatedTitle": "删除隔离的自定义供应商",
    "settings.providers.removeIsolatedDescription": "将永久删除该供应商的独立 Manifest、中央引用、连接、凭据与模型设置；其他供应商不受影响。",
    "settings.providers.permanentlyDelete": "永久删除",
    "settings.providers.customTitle": "自定义供应商",
    "settings.providers.addProvider": "添加供应商",
    "settings.providers.loading": "正在加载供应商",
    "settings.providers.emptyCustom": "尚未添加自定义供应商。",
    "settings.providers.table.provider": "供应商",
    "settings.providers.table.endpoint": "接口",
    "settings.providers.table.resources": "资源数量",
    "settings.providers.table.status": "状态",
    "settings.providers.table.priority": "优先级",
    "settings.providers.table.actions": "操作",
    "settings.providers.table.details": "详情",
    "settings.providers.table.configurationStatus": "配置状态",
    "settings.providers.table.operation": "操作",
    "settings.providers.table.protocolUnavailable": "未声明调用协议",
    "settings.providers.credentialCount": "{count} 个凭据",
    "settings.providers.configured": "已配置",
    "settings.providers.notConfigured": "未配置",
    "settings.providers.addCredentialFor": "为 {provider} 添加凭据",
    "settings.providers.editFor": "编辑 {provider}",
    "settings.providers.removeFor": "移除 {provider}",
    "settings.providers.priorityAria": "供应商优先级",
    "settings.providers.savePriority": "保存",
  },
  "en-US": {
    "ui.close": "Close",
    "ui.loading": "Loading",
    "settings.modelExperience.invalidValue": "The current value is outside the declared capabilities",
    "settings.modelExperience.extensionSelectionHint": "Choose native extension models that AI can discover and call as needed. Select an account for each model.",
    "settings.modelExperience.retainedEmbeddingInvalid": "A retained embedding dimension or batch size is invalid. Switch to vector retrieval and correct it before saving.",
    "gitRepository.list": "Git repositories",
    "gitRepository.primary": "Primary",
    "gitRepository.nested": "Nested",
    "gitRepository.branchUnknown": "Branch unavailable",
    "gitRepository.changesUnknown": "Changes unavailable",
    "gitRepository.changedFiles": "{count} changed files",
    "gitRepository.refreshing": "Refreshing",
    "gitRepository.readFailed": "Read failed",
    "gitRepository.busy": "Action in progress",
    "gitRepository.loading": "Loading repository status…",
    "gitRepository.unavailable": "Repository status unavailable",
    "gitRepository.retryReview": "Open review and retry",
    "projectOverview.conversations": "{count} conversations",
    "projectOverview.pinned": "Pinned",
    "projectOverview.issues": "Needs attention",
    "projectOverview.memoryLoading": "Loading memory settings…",
    "projectOverview.memoryUnknown": "Memory settings unavailable",
    "projectOverview.native": "Local native",
    "projectOverview.external": "External service",
    "projectOverview.enabled": "Enabled in settings",
    "projectOverview.disabled": "Disabled in settings",
    "projectOverview.override": "Project override",
    "projectOverview.inherited": "Inherited from global settings",
    "projectOverview.unsaved": "Unsaved changes",
    "projectOverview.missingOwner": "Memory owner #{id} is absent from the current catalog",
    "projectOverview.ownerUnknown": "Owner absent from catalog",
    "projectOverview.nativeBinding": "{name} (#{id}) · {count} linked recall projects",
    "projectOverview.unbound": "No memory project bound",
    "projectOverview.bindingUnknown": "Binding information unavailable",
    "projectOverview.externalBinding": "External memory project #{id}",
    "projectOverview.externalUnbound": "No external memory project bound",
    "projectOverview.precheck": "Precheck injection",
    "projectOverview.writeback": "Turn writeback",
    "projectOverview.on": "On",
    "projectOverview.off": "Off",
    "projectOverview.session": "Current session",
    "projectOverview.scopeReady": "Memory scope ready",
    "projectOverview.scopeNotReady": "Memory scope not ready",
    "projectOverview.precheckReady": "Precheck ready",
    "projectOverview.precheckNotReady": "Precheck not ready",
    "projectOverview.postactionReady": "Postaction ready",
    "projectOverview.postactionNotReady": "Postaction not ready",
    "projectOverview.runtimeUnknown": "No current-session runtime snapshot for this project",
    "projectOverview.memorySettings": "Memory settings",
    "projectOverview.codeSettings": "Code semantic index settings",
    "projectOverview.knowledgeSettings": "Knowledge settings",
    "projectOverview.skillSettings": "Skill settings",
    "projectOverview.indexDisabled": "Code context disabled",
    "projectOverview.queryReady": "Code context searchable",
    "projectOverview.queryUnavailable": "Code context currently unavailable",
    "projectOverview.indexUnknown": "Index status unavailable",
    "projectOverview.watching": "Watching source files",
    "projectOverview.notWatching": "Not watching source files",
    "projectOverview.pending": "Source updates pending",
    "projectOverview.files": "{count} files",
    "projectOverview.routesUnavailable": "Model routes not ready",
    "projectOverview.knowledgeCount": "{total} authorized libraries · {enabled} enabled by default",
    "projectOverview.knowledgeFailed": "Knowledge bindings could not be loaded",
    "projectOverview.knowledgeLoading": "Loading knowledge bindings…",
    "projectOverview.skillsLoading": "Loading skill overrides…",
    "projectOverview.skillsUnknown": "Skill overrides unavailable",
    "projectOverview.skillsCount": "{count} project skill overrides",
    "projectOverview.skillsInherited": "Skills inherit global settings",
    "projectOverview.maintenance": "Attachment statistics load on demand. Preview the impact before cleaning up.",
    "projectOverview.attachments": "Inspect and clean attachments",
    "projectOverview.details": "Project directory and storage details",
    "projectOverview.path": "Project directory",
    "projectOverview.copyPath": "Copy project path",
    "projectOverview.copy": "Copy path",
    "projectOverview.copied": "Copied",
    "projectOverview.copyFailed": "Copy failed; select the path text to copy it",
    "projectOverview.attachmentPath": "Attachment directory",
    "projectOverview.id": "Project ID",
    "settings.licenses.title": "About and licenses",
    "settings.licenses.types": "license types",
    "settings.licenses.dependencies": "dependencies",
    "settings.licenses.scope": "This catalog covers Rust desktop application dependencies. Frontend and Lua dependencies are not yet included.",
    "settings.licenses.search": "Search licenses, dependencies, versions or sources",
    "settings.licenses.reload": "Reload",
    "settings.licenses.loadError": "Could not read license resources",
    "settings.licenses.loading": "Loading license catalog…",
    "settings.licenses.emptySearch": "No matching licenses or dependencies.",
    "settings.licenses.unknown": "Unknown license",
    "settings.licenses.variants": "original bodies",
    "settings.licenses.dependency": "Dependency",
    "settings.licenses.version": "Version",
    "settings.licenses.source": "Source",
    "settings.licenses.body": "Body",
    "settings.licenses.view": "View",
    "settings.licenses.previous": "Previous dependency page",
    "settings.licenses.next": "Next dependency page",
    "settings.licenses.variant": "Select original body",
    "settings.licenses.variantHint": "Licenses of the same type may carry different copyright notices; each original body retains its actual dependencies.",
    "settings.licenses.retry": "Retry reading",
    "settings.licenses.loadingBody": "Loading original body…",
    "settings.modelExperience.retryLoad": "Reload settings",
    "settings.modelExperience.context": "Context and compaction",
    "settings.modelExperience.conversation": "Conversation defaults",
    "settings.modelExperience.nodes": "Compaction window nodes",
    "settings.modelExperience.retry": "Requests and retries",
    "settings.modelExperience.media": "Media delivery",
    "settings.modelExperience.usage": "Usage refresh",
    "settings.modelExperience.scopeHint": "Saving changes only system defaults. Provider, model, and conversation overrides remain independent.",
    "settings.modelExperience.inheritWindow": "Built-in default: 95% of the raw model window",
    "settings.modelExperience.inheritCompact": "Built-in default: 90% of the raw model window",
    "settings.modelExperience.inheritToken": "Built-in default: calculate by percentage, with no fixed token limit",
    "settings.modelExperience.inheritScope": "Built-in default: total usage",
    "settings.modelExperience.inheritBuffer": "Built-in default: 0 tokens; used only when a fallback prompt exists",
    "settings.modelExperience.savingSystem": "Saving system defaults…",
    "settings.modelExperience.dirtySystem": "System defaults have unsaved changes",
    "settings.modelExperience.savedSystem": "Current system defaults are saved",
    "settings.modelExperience.savingNodes": "Saving nodes…",
    "settings.modelExperience.dirtyNodes": "Compaction window nodes have unsaved changes",
    "settings.modelExperience.savedNodes": "Current node configuration is saved",
    "settings.modelExperience.nodeIds": "Fixed node identifiers",
    "settings.modelExperience.statistics": "View usage statistics and pricing",
    "settings.modelExperience.searchCustom": "Search custom providers",
    "settings.modelExperience.searchProvider": "Search providers",
    "settings.modelExperience.searchPlaceholder": "Search provider name or identifier",
    "settings.modelExperience.noProviders": "No matching providers",
    "settings.modelExperience.providerDetails": "Access and protocol details",
    "settings.modelExperience.usageFilter": "Filter model usage",
    "settings.modelExperience.allUsages": "All uses",
    "settings.modelExperience.conversationEnabled": "Conversation use enabled",
    "settings.modelExperience.internalEnabled": "Plugin calls enabled",
    "settings.modelExperience.multimodalEnabled": "Understanding candidate enabled",
    "settings.modelExperience.capabilityFilter": "Filter model capabilities",
    "settings.modelExperience.allCapabilities": "All capabilities",
    "settings.modelExperience.routes": "Access details · {count} routes",
    "settings.modelExperience.modelColumn": "Model",
    "settings.modelExperience.understandingUse": "Understanding",
    "settings.modelExperience.accessButton": "Access",
    "settings.modelExperience.candidateExplanation": "Allow as an image, video, or audio understanding candidate. Choose defaults in the corresponding understanding settings.",
    "settings.modelExperience.routeSettings": "Conditions",
    "settings.modelExperience.bindingSource": "Binding source",
    "settings.modelExperience.routeAccounts": "Credential accounts for this route",
    "settings.modelExperience.available": "Available",
    "settings.modelExperience.accountNotAdmitted": "Model disabled for this account",
    "settings.modelExperience.internalPurposes": "Call purposes: {purposes}",
    "settings.modelExperience.candidateHint": "Understanding candidate enabled. Select default models independently in understanding settings.",
    "settings.modelExperience.noConversation": "Conversation responses unavailable",
    "settings.modelExperience.unavailableCandidates": "Unavailable understanding candidates",
    "settings.modelExperience.disableCandidate": "Disable this candidate",
    "settings.modelExperience.curationModel": "Curation model",
    "settings.modelExperience.embeddingModel": "Embedding model",
    "settings.modelExperience.modelSearch": "Search model, provider, or account",
    "settings.modelExperience.curationNoMatch": "No matching curation models",
    "settings.modelExperience.modelNoMatch": "No matching models",
    "knowledge.experience.targetChanged": "The target library has changed. Cancel and review the replacement again.",
    "knowledge.experience.createTitle": "Create library",
    "knowledge.experience.retrievalMode": "Retrieval mode",
    "knowledge.experience.rawMode": "Raw library BM25 (no model cost)",
    "knowledge.experience.displayMode": "LLM-curated display library + BM25",
    "knowledge.experience.hybridMode": "LLM-curated display library + BM25 + vectors",
    "knowledge.experience.reasoning": "Reasoning effort",
    "knowledge.experience.reasoningPlaceholder": "Select a model-declared reasoning effort",
    "knowledge.experience.reasoningHint": "The model-declared effort is saved with this library and used for curation requests.",
    "knowledge.experience.dimensions": "Embedding dimensions",
    "knowledge.experience.dimensionsPlaceholder": "Select model-declared dimensions",
    "knowledge.experience.dimensionsInput": "Enter dimensions supported by the model",
    "knowledge.experience.dimensionsInvalid": "Dimensions are outside the current model's capabilities",
    "knowledge.experience.batch": "Embedding batch size (declared limit: {limit})",
    "knowledge.experience.batchFallback": "Not declared; default is 10",
    "knowledge.experience.compatibility": "Same-dimension compatibility confirmation (optional)",
    "knowledge.experience.compatibilityPlaceholder": "Explain why the existing index can be reused after changing models without changing dimensions",
    "knowledge.experience.compatibilityHint": "Providing a reason retains the existing index and displays the risk. Different dimensions still disable vector retrieval.",
    "knowledge.experience.compatibilityActive": "Same-dimension compatibility was confirmed manually; the existing vector index is reused: {reason}",
    "knowledge.experience.browseCorpus": "Browse corpus",
    "knowledge.experience.rawLibrary": "Raw library",
    "knowledge.experience.displayLibrary": "Display library",
    "knowledge.experience.searchLayer": "Search corpus",
    "knowledge.experience.rawSearch": "Raw library BM25",
    "knowledge.experience.displaySearch": "Display library BM25",
    "knowledge.experience.hybridSearch": "Display library hybrid search",
    "knowledge.experience.viewProgress": "View generation progress",
    "knowledge.experience.resume": "Resume generation",
    "knowledge.experience.displayReady": "Display library ready",
    "knowledge.experience.regenerate": "Regenerate",
    "knowledge.experience.buildDisplay": "Build display library",
    "knowledge.experience.buildVectors": "Build vectors",
    "knowledge.experience.saveSettings": "Save library settings",
    "knowledge.experience.configure": "Configure",
    "knowledge.experience.settingsTitle": "Library models and retrieval settings",
    "knowledge.experience.displayOperation": "Build library display layer",
    "knowledge.experience.vectorOperation": "Build library vector index",
    "knowledge.experience.background": "Run in background",
    "knowledge.experience.curating": "Curate display library",
    "knowledge.experience.curatingNow": "Curating display library",
    "knowledge.experience.embeddingNow": "Building vector index",
    "knowledge.experience.curatingProgress": "Curating display library {done} / {total} ({percent}%)",
    "knowledge.experience.embeddingProgress": "Building vectors {done} / {total} ({percent}%)",
    "knowledge.experience.cancelling": "Requesting cancellation",
    "knowledge.experience.failed": "Operation failed",
    "knowledge.experience.cancelled": "Operation cancelled",
    "knowledge.experience.completed": "Operation completed",
    "knowledge.experience.running": "Running",
    "knowledge.experience.waiting": "Waiting for operation",
    "knowledge.experience.processing": "Processing",
    "knowledge.experience.items": "{done} / {total} items",
    "knowledge.experience.llmRequests": "LLM requests",
    "knowledge.experience.embeddingRequests": "Embedding requests",
    "knowledge.experience.retries": "Retries {count}",
    "knowledge.experience.generated": "Generated {count}",
    "knowledge.experience.inputTokens": "Input tokens",
    "knowledge.experience.outputTokens": "Output tokens",
    "knowledge.experience.cumulative": "Cumulative",
    "knowledge.experience.cacheTokens": "Cache read / write",
    "knowledge.experience.currentRequest": "Current request",
    "knowledge.experience.noRequest": "No request started",
    "knowledge.experience.requestTokens": "Input {input} / output {output}",
    "knowledge.experience.preparing": "File scanning or index preparation is in progress; no LLM stream is available yet.",
    "knowledge.experience.waitingStream": "Waiting for JSON stream…",
    "knowledge.experience.recentRequests": "Recent requests (up to 3)",
    "knowledge.experience.noHistory": "No completed requests yet.",
    "knowledge.experience.currentFile": "Current file: ",
    "knowledge.experience.unknown": "Not provided",
    "settings.modelExperience.embeddingNoMatch": "No matching embedding models",
    "settings.modelExperience.curationUnavailable": "Selected curation model unavailable; select another",
    "settings.modelExperience.embeddingUnavailable": "Selected embedding model unavailable; select another",
    "settings.modelExperience.chooseCuration": "Select a curation model",
    "settings.modelExperience.chooseEmbedding": "Select an embedding model",
    "settings.modelExperience.noCuration": "No available curation models",
    "settings.modelExperience.noEmbedding": "No available embedding models",
    "memory.workspace.read": "Browse",
    "memory.workspace.purpose": "Purpose",
    "memory.workspace.actions": "Actions",
    "memory.workspace.manage": "Manage",
    "memory.workspace.settings": "Settings",
    "memory.workspace.service": "Models and service",
    "memory.workspace.browse": "Sessions and memories",
    "memory.workspace.search": "Semantic search",
    "memory.workspace.searchScope": "Search uses the current workspace project recall scope, independently of the memory project selected for browsing.",
    "memory.workspace.filters": "Filters",
    "memory.workspace.all": "All",
    "memory.workspace.bound": "Bound",
    "memory.workspace.unbound": "Unbound",
    "memory.workspace.overrides": "Overrides",
    "memory.workspace.noMatches": "No matching results",
    "memory.workspace.selectedCount": "{count} selected",
    "memory.workspace.clearFilters": "Clear filters",
    "memory.workspace.dateHint": "Filters recent activity in local time; the end date includes the entire day.",
    "memory.workspace.updatesAvailable": "Sessions have changed. Refresh to view updates.",
    "memory.workspace.missingOutputs": "These referenced outputs are absent from this snapshot. Refresh to check their status:",
    "memory.workspace.missingSource": "The source turn is absent from this snapshot. It may have been removed or be outside the current query scope.",
    "memory.workspace.disabled": "Memory is disabled in this scope",
    "memory.workspace.not_ready": "Memory is not ready; session policies remain editable",
    "memory.workspace.ready": "Memory is ready",
    "memory.workspace.selectSession": "Select a session to read its conversation, extraction results, and retained content.",
    "memory.workspace.editScope": "Editing scope",
    "memory.workspace.basic": "Enablement and automation",
    "memory.workspace.globalPolicy": "Global processing rules",
    "memory.workspace.recallPolicy": "Recall & extraction",
    "memory.workspace.textPolicy": "Text cleanup",
    "memory.workspace.profilePolicy": "Profile lifetimes",
    "memory.workspace.rankingPolicy": "Retrieval ranking",
    "memory.workspace.retentionPolicy": "Retention",
    "memory.advanced.catalogLoading": "Loading memory projects…",
    "memory.advanced.catalogFailed": "The memory project directory could not be loaded.",
    "memory.advanced.catalogRetry": "Reload directory",
    "memory.workspace.scopeHint": "Choose a scope first. Global defaults do not replace existing project overrides.",
    "memory.workspace.recordInfo": "Status and record information",
    "memory.workspace.projectRules": "Project overrides",
    "memory.workspace.effectiveFiles": "Active composition",
    "memory.workspace.otherFiles": "Other resource files",
    "memory.workspace.index": "Search indexes",
    "memory.workspace.cleanup": "Cleanup status",
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
    "area.contextEngine": "Code semantic index",
    "area.knowledge": "Knowledge",
    "area.settings": "Settings",
    "area.chat.description": "Project sessions, message stream, and model input",
    "area.skills.description": "LuaSkills skills, plugins, and runtime settings",
    "area.agentSkills.description": "Standard Agent Skills catalog",
    "area.agents.description": "Agents, expert profiles, and delegation prompts",
    "area.memory.description": "VMM user, project, and conversation memory",
      "area.contextEngine.description": "ContextEngine code-semantic indexing, model routes, and runtime status",
      "area.knowledge.description": "Project Markdown with Jieba BM25 search",
      "knowledge.name": "Knowledge library name",
      "knowledge.name.placeholder": "New knowledge library",
      "knowledge.create": "Create",
      "knowledge.filter.aria": "Filter knowledge libraries",
      "knowledge.filter.placeholder": "Filter libraries",
      "knowledge.refresh": "Refresh",
      "knowledge.emptyLibraries": "No knowledge libraries yet.",
      "knowledge.librariesLoading": "Loading knowledge libraries…",
      "knowledge.noMatchingLibraries": "No matching knowledge libraries.",
      "knowledge.readFailed": "Could not load this content. Refresh to retry.",
      "knowledge.libraryCounts": "{documents} documents · {chunks} chunks · {status}",
      "knowledge.indexStatus.empty": "No content",
      "knowledge.indexStatus.ready": "Index ready",
      "knowledge.indexStatus.dirty": "Index needs updating",
      "knowledge.indexStatus.rebuilding": "Rebuilding index",
      "knowledge.indexStatus.failed": "Index failed",
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
      "knowledge.composer.inherit": "Use project defaults",
      "knowledge.composer.loading": "Updating knowledge libraries…",
      "knowledge.composer.readFailed": "Knowledge selection could not be loaded.",
      "knowledge.composer.saveFailed": "Save not confirmed. Refresh the state before trying again.",
      "knowledge.composer.reloadFailed": "Selection saved. The latest state could not be loaded.",
      "knowledge.composer.retry": "Refresh state",
      "knowledge.composer.details": "View details",
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
    "appChrome.themeSaveFailed": "The theme is active for this session, but could not be saved on this client.",
    "appChrome.language.zh": "Chinese",
    "appChrome.scrollFeaturesLeft": "Scroll main features left",
    "appChrome.scrollFeaturesRight": "Scroll main features right",
    "appChrome.windowActionFailed": "Window action failed: ",
    "appChrome.window.minimize": "Minimize window",
    "appChrome.window.restore": "Restore window",
    "appChrome.window.maximize": "Maximize window",
    "appChrome.window.close": "Close window",
    "app.shutdown.persisting": "Saving conversation records and closing safely…",
    "app.shutdown.persistFailed.title": "Shutdown save did not finish",
    "app.shutdown.persistFailed.description": "The final conversation records could not be confirmed as durably saved. Cancel to keep the window open, then close it again to retry.",
    "app.shutdown.persistFailed.forceConfirm": "Forcing exit may lose the final conversation records. Force exit anyway?",
    "app.shutdown.persistFailed.cancel": "Cancel and keep window open",
    "app.shutdown.persistFailed.forceClose": "Force exit",
    "app.shutdown.persistFailed.forceClosing": "Forcing exit…",
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
    "conversation.submission.uploadFailed": "Attachment upload failed",
    "conversation.submission.uploadFailedAttachmentKept": "The file remains attached. Its content was not sent in this turn.",
    "conversation.submission.uploadProgress": "Media upload progress",
    "conversation.submission.sizeUnknown": "The total file size is not available yet",
    "conversation.submission.cancelFailed": "Could not cancel the upload. Please retry.",
    "conversation.submission.cancelling": "Cancelling upload",
    "conversation.loading.requesting": "Requesting conversation data",
    "conversation.loading.locating": "Locating conversation",
    "conversation.loading.index": "Reading message index",
    "conversation.loading.reading": "Reading message content",
    "conversation.loading.subagents": "Restoring open agent index",
    "conversation.loading.projecting": "Preparing display data",
    "conversation.loading.done": "Conversation loaded",
    "conversation.loading.failed": "Conversation body failed to load",
    "conversation.loading.retry": "Retry loading",
    "conversation.unlinkedProject": "Unlinked project",
    "conversation.history.loadMore": "Load more history",
    "conversation.history.loading": "Loading history",
    "conversation.history.invalidResponse": "The history page belongs to a different conversation and was rejected.",
    "conversation.history.invalidCursor": "The history page did not advance to an older position and was rejected to prevent repeated loading. Retry and inspect the conversation history page diagnostics.",
    "conversation.history.emptyPageWithMore": "No history was loaded, but the service still reports older records. Retry and inspect the conversation history page diagnostics.",
    "conversation.history.loadFailed": "Failed to load older history",
    "conversation.action.copyFinal": "Copy final output source",
    "conversation.action.copiedFinal": "Copied final output source",
    "conversation.action.fork": "Fork conversation",
    "conversation.action.forkTitleSuffix": " (fork)",
    "conversation.action.forkFailed": "Fork failed",
    "conversation.action.forkSucceeded": "Forked conversation created",
    "conversation.action.forkInProgressTitle": "Forking conversation",
    "conversation.action.forkInProgressMessage": "Copying the conversation and opening the new fork. Please wait.",
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
    "conversation.systemError.pendingTitle": "Result and usage pending",
    "conversation.systemError.pendingStatus": "Pending",
    "conversation.systemError.pendingNotice": "The provider accepted this request but has not confirmed its final result or usage. Vulcan will not resend it automatically, avoiding duplicate execution or billing; use the request identifiers to reconcile it in provider history.",
    "conversation.systemError.resultConfirmation": "Result status",
    "conversation.systemError.usageConfirmation": "Usage status",
    "conversation.systemError.unconfirmed": "Unconfirmed",
    "conversation.systemError.requestIdentifier": "Request identifier",
    "conversation.systemError.requestIdentifierSource": "Identifier source",
    "conversation.systemError.upstreamErrorPayload": "Upstream error payload",
    "conversation.systemError.identifierMissing": "Not provided by provider",
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
    "conversation.providerRetry.retrying": "Retrying",
    "conversation.providerRetry.succeeded": "Retry succeeded",
    "conversation.providerRetry.failed": "Retry failed",
    "conversation.modelSwitch.title": "Model changed",
    "conversation.modelSwitch.changed": "Model changed from {previous} to {current}.",
    "conversation.goalNotice.saved": "Goal saved",
    "conversation.goalNotice.resumed": "Goal resumed",
    "conversation.goalNotice.paused": "Goal paused",
    "conversation.goalNotice.blocked": "Goal blocked; automatic continuation paused",
    "conversation.goalNotice.completed": "Goal completed",
    "conversation.goalNotice.cleared": "Goal removed",
    "conversation.goalNotice.budgetExhausted": "Goal budget exhausted; paused",
    "conversation.goalNotice.usageLimited": "Goal usage limited; paused",
    "conversation.goalNotice.usageUnknown": "Goal usage unavailable; paused",
    "conversation.goalNotice.updated": "Goal status updated",
    "composer.draft.retry": "Retry loading draft",
    "conversation.modelSwitch.help": "About model changes",
    "conversation.modelSwitch.warning": "Switching models during a conversation may reduce performance. Background context may be compacted automatically.",
    "conversation.reasoningSwitch.title": "Reasoning effort",
    "conversation.reasoningSwitch.nextProviderRequest": "Reasoning effort changed from {previous} to {current}; the current Provider request remains frozen, and the new effort applies to the next real Provider request.",
    "conversation.continue": "Continue",
    "conversation.continuing": "Continuing",
    "conversation.continuation.claimFailed": "Continue failed. Check the session state and try again.",
    "conversation.continuation.blocked": "This terminal boundary is not safe to continue",
    "conversation.continuation.blocked.default": "The open turn did not pass the continuation eligibility checks.",
    "conversation.continuation.blocked.activeApi": "A physical API request is still active, so Continue cannot be admitted again.",
    "conversation.continuation.blocked.missingApiStarted": "The open turn has no related physical API start fact and cannot continue.",
    "conversation.continuation.blocked.duplicateApiTerminal": "The physical API has duplicate terminal facts and cannot continue.",
    "conversation.continuation.blocked.missingApiTerminal": "The physical API has no durable terminal fact yet and cannot continue.",
    "conversation.continuation.blocked.apiTerminalOrder": "The physical API terminal order is invalid and cannot continue.",
    "conversation.continuation.blocked.terminalBoundarySuperseded": "A later turn fact superseded this terminal boundary. Refresh the conversation state.",
    "conversation.continuation.blocked.terminalPolicyForbidden": "The durable terminal fact for this physical API explicitly forbids continuation.",
    "conversation.continuation.blocked.routeChanged": "The composer model or connection does not match the session's next-request configuration.",
    "conversation.continuation.blocked.sessionBusy": "Another execution currently owns this session, so it cannot continue.",
    "conversation.continuation.blocked.requestInvalid": "The Continue request has an invalid client action identity and was rejected.",
    "conversation.continuation.blocked.conversationUnavailable": "The target conversation is missing, invalid, or is not an existing continuable session.",
    "conversation.continuation.blocked.runtimeUnavailable": "The V1 runtime owner for the target conversation is not ready.",
    "conversation.continuation.blocked.unknown": "The current V1 domain contract rejected the Continue request.",
    "conversation.continuation.timeout": "Continue timed out. The button was unlocked; check the session state.",
    "conversation.continuation.stateSyncFailed": "The service accepted Continue, but the UI state did not synchronize. Retry the same button to reconcile; it will not create a second execution.",
    "conversation.continuation.ackMismatch": "The Continue response did not match this request identity. The UI rejected it and reloaded authoritative state.",
    "conversation.metadata.type": "Type",
    "conversation.metadata.status": "Status",
    "conversation.metadata.tool": "Tool",
    "conversation.metadata.identifier": "Identifier",
    "conversation.metadata.error": "Error",
    "conversation.metadata.result": "Result",
    "conversation.metadata.returned": "Returned",
    "conversation.toolRequest.receiving": "Receiving data",
    "conversation.toolRequest.argumentsAssembled": "Arguments assembled",
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
    "conversation.process.working": "Working",
    "conversation.process.worked": "Worked",
    "conversation.process.stoppedAfter": "You stopped after {duration}",
    "conversation.process.stopped": "You stopped this turn",
    "conversation.process.duration": "Duration {duration}",
    "conversation.process.providerUsageBadge": "Current session Provider API {calls} · {inFlight} in progress · Tokens {tokens}",
    "conversation.process.providerUsageSession": "Current session: {calls} calls, tokens {tokens}, unknown Usage {unknown}",
    "conversation.process.providerUsageUnknown": "unknown",
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
    "composer.capability.loading": "Loading session capabilities…",
    "composer.capability.loadFailed": "Failed to load session capabilities",
    "composer.capability.retry": "Reload",
    "composer.capability.unavailableEnabled": "The saved activation cannot take effect. Choose an available target or turn off this capability.",
    "composer.capability.disableUnavailable": "Turn off and keep selection",
    "composer.capability.removeUnavailable": "Remove selection",
    "composer.capability.unavailableDetails": "View saved model and account",
    "composer.capability.operationFailed": "Settings were not saved. Please try again.",
    "composer.capability.errorDetails": "View error details",
    "composer.capability.rebuild": "Rebuild default settings",
    "composer.capability.rebuilding": "Rebuilding…",
    "composer.capability.rebuildHint": "Reload, or rebuild this conversation's capability settings from global defaults. Rebuilding clears custom capability choices without changing history, goals, or other settings.",
    "composer.capability.tooltipEmpty": "No enabled modules",
    "composer.capability.dialogTitle": "Current session capabilities",
    "composer.capability.dialogDescription": "Choose this session's capabilities and execution targets. Saved choices apply to subsequent turns.",
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
    "composer.contextWindow.title": "Compaction window",
    "composer.contextWindow.default": "Default (follow model)",
    "composer.contextWindow.invalid": "Unavailable for this model",
    "composer.contextWindow.locked": "The compaction window is locked for this session.",
    "composer.contextWindow.defaultRestored": "The default model compaction window is restored.",
    "composer.contextWindow.previewUnavailable": "The preview session cannot save a compaction window.",
    "composer.contextWindow.saveFailed": "The compaction window was not saved. Please retry.",
    "composer.modelPicker.filter": "Filter by model name or ID",
    "composer.modelPicker.noMatches": "No models match the current filter.",
    "composer.attachment.add": "Add attachment",
    "composer.attachment.extension": "Extension",
    "composer.attachment.menu": "Attachments",
    "composer.attachment.empty": "No attachment types are available",
    "composer.attachment.remove": "Remove attachment",
    "composer.attachment.processing": "Confirming uploaded resource",
    "composer.attachment.preparationBlocked": "{count} attachment(s) are still being prepared. Wait for completion before sending.",
    "composer.attachment.metadataOnly": "This attachment will be disclosed as metadata only and will not be sent for direct model understanding.",
    "composer.attachment.details": "View attachment details",
    "composer.attachment.failed": "Attachment preparation failed",
    "composer.attachment.retry": "Retry upload",
    "composer.attachment.unsupported": "These attachments could not be staged.",
    "composer.attachment.maxLimit": "You can add at most {count} attachments.",
    "composer.attachment.partialMaxLimit": "The {count}-attachment limit has been reached. Some files were not added.",
    "composer.attachment.kind.image": "Image",
    "composer.attachment.kind.audio": "Audio",
    "composer.attachment.kind.video": "Video",
    "composer.attachment.kind.pdf": "PDF",
    "composer.attachment.kind.file": "File",
    "composer.thinkingProcess": "Thinking popup",
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
  "composer.goal.status.blocked": "Blocked goal (paused)",
  "composer.goal.duration": "Working time",
  "composer.goal.usage": "Goal usage",
  "composer.goal.requests": "Requests",
  "composer.goal.tools": "Tool calls",
  "composer.goal.cacheHit": "Cache hit rate",
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
    "composer.error.draftLoadFailed": "Failed to load the conversation draft. Sending is paused.",
    "composer.error.draftPersistFailed": "Failed to save the conversation draft. Try again.",
    "composer.error.draftChangedBeforeSubmission": "The draft changed. Nothing was sent and your content was kept.",
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
    "composer.accountCard.commandCodeMonthlyCredits": "Monthly credits",
    "composer.accountCard.commandCodePurchasedCredits": "Purchased credits",
    "composer.accountCard.commandCodeFreeCredits": "Free credits",
    "composer.accountCard.commandCodeFiveHourRequests": "5-hour requests",
    "composer.accountCard.commandCodeWeeklyRequests": "Weekly requests",
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
    "conversationActions.previewCannotContinue": "Real conversations cannot be continued in browser preview mode.",
    "conversationActions.modelSwitched": "Selected {model} ({provider}); this selection applies to the next real Provider request.",
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
    "projectSettings.knowledgeBasesSavedRefreshFailed": "Access was saved, but refreshing failed. Reload the current state.",
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
    "projectSettings.codeContext.title": "Code semantic index",
    "projectSettings.codeContext.enabled": "Native semantic index enabled",
    "projectSettings.codeContext.disabled": "Currently disabled",
    "projectSettings.codeContext.generating": "Index task in progress; follow the progress below.",
    "projectSettings.vmmDescription": "Override memory enablement, user/project mapping, and recall injection parameters for this project.",
    "projectSettings.vmmNativeDescription": "Override memory enablement, native project binding, and recall injection parameters for this project.",
    "projectSettings.vmmLoading": "Loading project VMM settings...",
    "projectSettings.enableProjectOverride": "Enable project override",
    "projectSettings.overrideEnabledHint": "After saving, this project uses the settings below.",
    "projectSettings.overrideDisabledHint": "Switch off and save to use global settings.",
    "projectSettings.vmmTransportHint": "Project-level settings inherit the global gRPC connection and API Key; adjust connection parameters in global VMM memory settings.",
    "projectSettings.vmmNativeHostHint": "Native memory uses the local data directory and configured AI capabilities.",
    "projectSettings.vmmNativeEnabled": "Enable native memory",
    "projectSettings.vmmAdvanced": "Advanced injection settings",
    "projectSettings.saveVmm": "Save VMM settings",
    "projectSettings.vmmNativeBinding": "Native memory project binding",
    "projectSettings.vmmNativeOwner": "Write owner project",
    "projectSettings.vmmNativeLinked": "Additional recall projects (read only)",
    "projectSettings.vmmNativeSaveBinding": "Save native binding",
    "projectSettings.vmmCreateBinding": "Create a new memory project binding",
    "projectSettings.vmmCreateAndBind": "Create and bind",
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
    "workspaceAudit.connecting": "Connecting to this terminal…",
    "workspaceAudit.terminalFailure": "Terminal error: {error}",
    "workspaceAudit.exited": "Terminal exited",
    "workspaceAudit.exitedWithCode": "Terminal exited (code {code})",
    "workspaceAudit.stopped": "Terminal stopped",
    "workspaceAudit.failed": "Terminal failed to start or run",
    "workspaceAudit.copyTruncated": "Preview copied; the long file content was truncated.",
    "workspaceAudit.retry": "Retry",
    "workspaceAudit.viewportLoading": "Loading terminal component…",
    "workspaceAudit.aiReadOnly": "AI controlled · Read-only display",
    "workspaceAudit.restoreAi": "Restore AI terminal window",
    "workspaceAudit.maximizeAi": "Maximize AI terminal window",
    "workspaceAudit.minimizeAi": "Minimize AI terminal",
    "workspaceAudit.startupSteps": "{completed}/{total} steps completed",
    "workspaceAudit.terminalLoading": "Loading project terminals…",
    "workspaceAudit.emptyName": "Terminal name cannot be empty.",
    "workspaceAudit.terminal": "Project terminal",
    "workspaceAudit.expandList": "Expand project terminal list",
    "workspaceAudit.collapseList": "Collapse project terminal list",
    "workspaceAudit.restore": "Restore project terminal window",
    "workspaceAudit.maximize": "Maximize project terminal window",
    "workspaceAudit.minimize": "Minimize project terminal",
    "workspaceAudit.numberedList": "Numbered project terminals",
    "workspaceAudit.switchTerminal": "Switch to terminal {index}: {name}",
    "workspaceAudit.chooseShell": "Select shell for a new terminal",
    "workspaceAudit.create": "Create project terminal",
    "workspaceAudit.list": "Project terminal list",
    "workspaceAudit.name": "Terminal name",
    "workspaceAudit.saveName": "Save name for {name}",
    "workspaceAudit.editName": "Edit name for {name}",
    "workspaceAudit.stop": "Stop {name}",
    "workspaceAudit.close": "Close {name}",
    "workspaceAudit.stopTitle": "Stop terminal",
    "workspaceAudit.closeTitle": "Close terminal",
    "workspaceAudit.empty": "Choose a shell, then select + to create a project terminal",
    "workspaceAudit.closeConfirm": "Close the running terminal?",
    "workspaceAudit.stopConfirm": "Stop this terminal?",
    "workspaceAudit.closeDescription": "“{name}” is running. Confirmation stops its entire process tree and removes this terminal.",
    "workspaceAudit.stopDescription": "Confirmation stops the entire process tree of “{name}” and keeps its output tab.",
    "workspaceAudit.cancel": "Cancel",
    "workspaceAudit.processing": "Processing…",
    "workspaceAudit.confirm": "Confirm",
    "workspaceAudit.openDirectory": "Open directory: {path}",
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
    "workspaceShell.memoryTitle": "Memory workspace",
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
    "thinkingDisplay.popupOnly": "On",
    "thinkingDisplay.inlineOutput": "Off",
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
    "ask.custom.adjustCodex": "Other answer",
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
    "conversation.subagent.state.closed": "Closed",
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
    "conversation.subagent.detail.usage": "Agent usage",
    "conversation.subagent.detail.result": "Result",
    "conversation.subagent.detail.noResult": "No result content.",
    "conversation.subagent.detail.target": "Target",
    "conversation.subagent.detail.queued": "Queued",
    "conversation.subagent.detail.steering": "Steering",
    "conversation.subagent.queue.steering": "Pending guidance",
    "conversation.subagent.queue.followup": "Queued follow-up",
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
"toolPresentation.output.loadFailed": "Unable to load the tool result",
"toolPresentation.output.retryLoad": "Reload",
"toolPresentation.output.errorDetails": "Error details",
    "toolPresentation.output.waiting": "Waiting for tool output...",
    "toolPresentation.output.empty": "No output",
    "toolPresentation.payload.input": "Input",
    "toolPresentation.payload.output": "Output",
    "toolPresentation.modelExtension.parameters": "Get model extension parameters",
    "toolPresentation.raw.open": "Show raw tool invocation output",
    "toolPresentation.raw.title": "Raw {tool} invocation",
    "toolPresentation.raw.legacyPartial": "Only the visible partial invocation is available.",
    "toolPresentation.execution.running": "Running",
    "toolPresentation.exitCode": "Exit code",
    "toolPresentation.command.terminalTitle": "Terminal command",
    "toolPresentation.command.shellTitle": "Shell command",
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
    "toolPresentation.native.todolist": "Update todo list",
    "toolPresentation.native.getGoal": "Read goal",
    "toolPresentation.native.createGoal": "Create goal",
    "toolPresentation.native.updateGoal": "Update goal",
    "toolPresentation.native.knowledgeSearch": "Search knowledge",
    "toolPresentation.native.knowledgeBrowse": "Browse knowledge",
    "toolPresentation.native.knowledgeRead": "Read knowledge document",
    "toolPresentation.native.skill": "View skill",
    "toolPresentation.native.memorySearch": "Search memory",
    "toolPresentation.native.turnDetails": "Read turn details",
    "toolPresentation.native.memoryWrite": "Write memory",
    "toolPresentation.native.memoryDelete": "Delete memory",
    "toolPresentation.native.runtimeConfig": "Runtime configuration",
    "toolPresentation.file.count": "{count} files",
    "toolPresentation.file.multiChange": "Multiple file changes",
    "toolPresentation.file.empty": "No displayable content",
    "toolPresentation.file.mixedLineEndings": "Mixed EOL",
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
    "toolPresentation.file.edited": "Edited",
    "toolPresentation.file.notEdited": "Not edited",
    "toolPresentation.file.executionFailed": "Execution failed",
    "toolPresentation.file.executionStopped": "Stopped",
    "toolPresentation.file.executionCompleted": "Completed",
    "toolPresentation.markdownOutput.running": "Querying",
    "toolPresentation.markdownOutput.failed": "Query failed",
    "toolPresentation.markdownOutput.stopped": "Stopped",
    "toolPresentation.markdownOutput.completed": "Queried",
    "toolPresentation.fileList.title": "List files",
    "toolPresentation.fileRead.title": "Read file",
    "toolPresentation.fileRead.rangeFrom": "From line {start}",
    "toolPresentation.fileRead.range": "Lines {start}-{end}",
    "toolPresentation.fileSearch.title": "Search files",
    "toolPresentation.fileStat.title": "Read file metadata",
    "toolPresentation.codekit.astTree.title": "List AST Tree",
    "toolPresentation.codekit.astDetail.title": "View AST Details",
    "toolPresentation.codekit.rg.title": "RG Enhanced Search",
    "toolPresentation.codekit.nodeSource.title": "Get node source",
    "toolPresentation.codekit.markdownMenu.title": "Query Markdown outline",
    "markdown.wrap.enable": "Enable line wrap",
    "markdown.wrap.disable": "Disable line wrap",
    "markdown.copyCode": "Copy code",
    "conversation.control.copyFailed": "Copy failed",
    "conversation.control.actionFailed": "Action failed",
    "conversation.control.detailFailed": "Could not load details",
    "conversation.control.retry": "Load again",
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
    "memory.overview.title": "Status overview",
    "memory.overview.description": "Review memory service status, complete initial setup, and resolve blocking issues.",
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
    "vmm.experience.quickSetup.description": "Enable long-term memory with recommended settings. Vulcan Code checks the selected backend, model configuration, and data safety before starting.",
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
    "vmm.experience.defaultMemoryMode": "Default memory mode",
    "vmm.experience.nativeBindingFixed": "Bind to a fixed memory space",
    "vmm.experience.nativeBindingPerProject": "Create one per project",
    "vmm.experience.refresh": "Refresh status",
    "vmm.experience.start": "Start service",
    "vmm.experience.restart": "Restart",
    "vmm.experience.stop": "Stop service",
    "vmm.experience.reconfigure": "Reconfigure memory backend",
    "vmm.experience.openAdvanced": "Open advanced settings",
    "memory.identity.title": "Users and projects",
    "memory.identity.description": "Manage durable memory users and project mappings. Internal identifiers remain hidden and are used only for routing.",
    "memory.identity.nativeTitle": "Memory projects",
    "memory.identity.nativeDescription": "Manage one-level native memory projects, VulcanCode ownership, linked recall, migration, clearing, and deletion.",
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
    "memory.rules.description": "Choose the editing scope, then adjust its memory rules.",
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
    "memory.rules.allProjects": "Global defaults",
    "memory.rules.oneProject": "One selected project",
    "memory.rules.selectProject": "Select a project",
    "memory.rules.searchProject": "Search projects…",
    "memory.rules.noProject": "No projects available",
    "memory.rules.applied": "Settings applied.",
    "memory.content.title": "Memory content",
    "memory.content.retrieval": "Retrieval details",
    "memory.content.finalScore": "Final retrieval score",
    "memory.content.creationSource": "Memory creation source",
    "memory.content.totalEvidence": "All contexts: support / rebuttal",
    "memory.content.matchedEvidence": "Matched: support / rebuttal",
    "memory.content.contextAdjustment": "Net context adjustment",
    "memory.content.contextNotObserved": "Context evidence was not evaluated for this query; stored evidence may still exist.",
    "memory.content.description": "Use natural language to search, inspect, and maintain durable memory for the current user and project.",
    "memory.content.nativeDescription": "Browse sessions and durable content in memory projects, or search within the current work project's recall scope.",
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
    "memory.nativeRecords.title": "Native memory provenance",
    "memory.sessionList.title": "Memory sessions",
    "memory.sessionList.description": "Filter session summaries by project, then open extraction records and generated memories. Selection applies only to this page.",
    "memory.sessionList.back": "Back to sessions",
    "memory.sessionList.projectSources": "View all project provenance",
    "memory.sessionList.search": "Session title or ID",
    "memory.sessionList.status": "Session status",
    "memory.sessionList.extraction": "Extraction status",
    "memory.sessionList.allStates": "All session states",
    "memory.sessionList.allExtraction": "All extraction states",
    "memory.sessionList.from": "Start date",
    "memory.sessionList.before": "End date",
    "memory.sessionList.sort": "Sort by",
    "memory.sessionList.newest": "Latest activity first",
    "memory.sessionList.oldest": "Earliest activity first",
    "memory.sessionList.titleSort": "Title order",
    "memory.sessionList.apply": "Apply filters",
    "memory.sessionList.loading": "Loading session summaries…",
    "memory.sessionList.failed": "Could not load sessions. Refresh and check the action notification for details.",
    "memory.sessionList.detailFailed": "Could not load provenance. Refresh and check the action notification for details.",
    "memory.sessionList.invalidDates": "Choose valid dates with the end on or after the start.",
    "memory.sessionList.selectPage": "Select this page",
    "memory.sessionList.selected": "{count} sessions selected",
    "memory.sessionList.mixed": "Archive requires all selected sessions to be active; unarchive requires all to be archived.",
    "memory.sessionList.select": "Select session",
    "memory.sessionList.empty": "No sessions match these filters.",
    "memory.sessionList.total": "{total} sessions · Page {page}",
    "memory.sessionList.previous": "Previous",
    "memory.sessionList.next": "Next",
    "memory.sessionArchive.active": "Active",
    "memory.sessionArchive.archived": "Archived",
    "memory.sessionArchive.recycled": "Recycled",
    "memory.sessionArchive.forgotten": "Forgotten",
    "memory.sessionArchive.unregistered": "Extraction session not yet registered",
    "memory.sessionArchive.archive": "Archive session",
    "memory.sessionArchive.unarchive": "Unarchive session",
    "memory.sessionArchive.description": "Only changes the management classification. Memories, profiles and vectors are preserved; recall and extraction remain enabled. This preview expires after five minutes and must be refreshed if records change.",
    "memory.sessionArchive.confirm": "Confirm change",
    "memory.removal.sessions": "Preview session recycling",
    "memory.removal.turns": "Preview selected turn recycling",
    "memory.removal.selectSession": "Select session for recycling",
    "memory.removal.selectTurn": "Select turn for recycling",
    "memory.removal.title": "Confirm move to memory recycle bin",
    "memory.removal.description": "Move the original records, associated ordinary memories and vectors to the recycle bin without copying or re-embedding. Keep all user and project profiles. VulcanCode conversations are not deleted.",
    "memory.removal.sessionEffect": "Recycling a whole memory session pauses its memory processing. Restore it from the recycle bin.",
    "memory.removal.turnEffect": "Recycle only selected turns and their ordinary memories. The memory session remains in its current enabled state.",
    "memory.removal.vectors": "Associated vectors",
    "memory.removal.hotTurns": "Affected hot-window turns",
    "memory.removal.hotWindow": "Current hot-window size",
    "memory.removal.context": "Associated context entries",
    "memory.removal.projectProfiles": "Related project profiles (kept)",
    "memory.removal.userProfiles": "Related user profiles (kept)",
    "memory.removal.retention": "Retained from execution for",
    "memory.removal.days": "days; cleanup may permanently purge expired records.",
    "memory.removal.expirySuffix": "; cleanup may permanently purge expired records.",
    "memory.rules.invalidNumbers": "Counts must be integers from 0 to 4294967295. Leave blank to inherit. Correct the marked inputs before saving.",
    "memory.rules.features": "Features and permissions",
    "memory.rules.retentionProfiles": "Retention and profiles",
    "memory.advanced.invalidNumbers": "Transport settings must be non-negative integers. Leave blank to inherit. Correct the marked inputs before saving.",
    "memory.projectProfile.durationHint": "Lifetime = base + refresh count × extension, capped by maximum retention. Base must be at least 1 ms and no greater than the maximum; extension may be zero; the limit is 3650 days. Each unit applies only to its adjacent value. Persistent profiles never expire.",
    "memory.removal.sources": "Inspect exact source sessions and turns",
    "memory.removal.confirm": "Confirm move to recycle bin",
    "memory.removal.failed": "Recycling did not complete. Inspect the action error and request a fresh preview. Expired or changed plans cannot be confirmed again.",
    "memory.sessionRestart.action": "Restart memory",
    "memory.sessionRestart.description": "Enable future memory reads and writes for these forgotten sessions only. Deleted memories, profiles and vectors are not restored, and historical conversations are not resent or re-extracted.",
    "memory.sessionRestart.confirm": "Confirm restart",
    "memory.sessionRestart.failed": "Memory was not restarted. Preview again and check the action notification for details.",
    "memory.sessionArchive.failed": "The archive operation did not complete. Preview again and check the action notification for details.",
    "memory.nativeRecords.description": "Inspect turn extraction state, memory levels, and exact provenance by native memory project and host conversation.",
    "memory.nativeRecords.project": "Memory project",
    "memory.nativeRecords.session": "Host conversation",
    "memory.nativeRecords.selectProject": "Select a memory project",
    "memory.nativeRecords.selectSession": "Select a host conversation",
    "memory.nativeRecords.sessionOverview": "Conversation overview",
    "memory.nativeRecords.refresh": "Refresh records",
    "memory.nativeRecords.noProjects": "No native memory project is available to inspect.",
    "memory.nativeRecords.noSessions": "This memory project has not produced any conversation records.",
    "memory.nativeRecords.noProjectRecords": "This memory project has no turn or memory records yet.",
    "memory.nativeRecords.sessions": "Conversation records",
    "memory.nativeRecords.turns": "Turns",
    "memory.nativeRecords.memories": "Memories",
    "memory.nativeRecords.profileNodes": "Profile nodes",
    "memory.nativeRecords.completed": "Extracted",
    "memory.nativeRecords.pending": "Pending extraction",
    "memory.nativeRecords.processedUnknown": "Processed, result unknown",
    "memory.nativeRecords.bypassed": "Bypassed",
    "memory.nativeRecords.corrupted": "Corrupted",
    "memory.nativeRecords.lastActivity": "Last activity",
    "memory.nativeRecords.projectSummary": "Turn statuses and memory levels across the entire memory project",
    "memory.nativeRecords.turnRecords": "Turn extraction records",
    "memory.nativeRecords.projectTurnRecords": "All project turn extraction records",
    "memory.nativeRecords.sessionMemories": "Memories generated or written by this conversation",
    "memory.nativeRecords.projectMemories": "All project memories",
    "memory.nativeRecords.projectOnly": "Project-owned without a source conversation",
    "memory.nativeRecords.noTurns": "This conversation has no canonical turn records.",
    "memory.nativeRecords.noMemories": "This conversation has not generated or written durable memory.",
    "memory.nativeRecords.generated": "Generated memories",
    "memory.nativeRecords.generatedMemories": "Memories generated by this turn",
    "memory.nativeRecords.noGeneratedMemories": "This turn did not generate durable memory.",
    "memory.nativeRecords.generatedProfiles": "Profiles generated by this turn",
    "memory.nativeRecords.noGeneratedProfiles": "This turn did not generate user or project profile nodes.",
    "memory.nativeRecords.noProfileNodes": "No user or project profile nodes were generated in the selected scope.",
    "memory.nativeRecords.noUserPreview": "This turn has no user content to display.",
    "memory.nativeRecords.retryCount": "Failed retry count",
    "memory.nativeRecords.sourceTurn": "Source turn",
    "memory.nativeRecords.priority": "Priority",
    "memory.nativeRecords.tags": "Tags",
    "memory.nativeRecords.statusReason": "Status reason",
    "memory.nativeRecords.expiresAt": "Expires",
    "memory.nativeRecords.turn": "Turn",
    "memory.nativeRecords.timeline": "Complete conversation content",
    "memory.nativeRecords.previousTurns": "Previous turns",
    "memory.nativeRecords.nextTurns": "Following turns",
    "memory.nativeRecords.analysisDetails": "Extraction details",
    "memory.nativeRecords.lastError": "Latest extraction error",
    "memory.nativeRecords.statusPending": "Pending extraction",
    "memory.nativeRecords.statusCompleted": "Extracted",
    "memory.nativeRecords.statusProcessedUnknown": "Processed; extraction result unknown",
    "memory.nativeRecords.statusBypassed": "Bypassed after analysis failures",
    "memory.nativeRecords.statusCorrupted": "Payload corrupted and isolated",
    "memory.nativeRecords.levelSession": "Session level",
    "memory.nativeRecords.levelPhase": "Phase level",
    "memory.nativeRecords.levelStable": "Stable level",
    "memory.nativeRecords.levelPersistent": "Persistent level",
    "memory.nativeRecords.stateActive": "Active",
    "memory.nativeRecords.stateSuperseded": "Superseded",
    "memory.nativeRecords.stateDeleted": "Deleted",
    "memory.nativeRecords.stateExpired": "Expired",
    "memory.nativeRecords.stateRecycled": "Recycled",
    "memory.nativeRecords.sourceTurnExtracted": "Turn extraction",
    "memory.nativeRecords.sourceDirect": "Direct write",
    "memory.nativeRecords.sourceImport": "Import",
    "memory.nativeRecords.sourceSystemSeed": "System seed",
    "memory.nativeRecords.sourceLegacy": "Legacy migration",
    "memory.nativeRecords.categoryGeneral": "General fact",
    "memory.nativeRecords.categoryArchitecture": "Architecture decision",
    "memory.nativeRecords.categoryTechSpec": "Technical specification/API",
    "memory.nativeRecords.categoryBusiness": "Business logic",
    "memory.nativeRecords.categoryRequirement": "Requirement/TODO",
    "memory.nativeRecords.categoryProject": "Project context",
    "memory.nativeRecords.categoryBugDebt": "Defect/technical debt",
    "memory.nativeRecords.categorySecurity": "Security policy",
    "memory.nativeRecords.roleUser": "User",
    "memory.nativeRecords.roleAssistant": "Assistant",
    "memory.nativeRecords.roleSystem": "System",
    "memory.nativeRecords.roleTool": "Tool",
    "memory.data.title": "Memory data",
    "memory.data.description": "Inspect long-term memory sources by session and turn, then archive, recycle, or restore data through an impact preview.",
    "memory.data.managementReady": "Management service ready",
    "memory.data.managementUnavailable": "Management service unavailable",
    "memory.data.extractionRuntimeTitle": "Memory extraction is waiting",
    "memory.data.extractionRuntimeWaiting": "The post-action extraction service is not ready yet. Durably stored turns will retry automatically when it recovers.",
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
    "memory.profile.nativeDescription": "Review the fixed local user profile or select a native memory project profile, then correct it with natural language.",
    "memory.profile.target": "Profile target",
    "memory.profile.user": "User profile",
    "memory.profile.project": "Project profile",
    "memory.profile.nativeUserName": "Local user",
    "memory.profile.nativeUserDescription": "Native memory has one fixed user managed by VulcanCode, so no user selection is required.",
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
    "memory.profile.invalidBundleMode": "The profile service did not return an explicit content mode. Refresh and check the service response.",
    "memory.profile.previewDescription": "This is the complete profile text supplied to the memory service. Open it only for review or troubleshooting.",
    "memory.profile.nativePreviewDescription": "Profile text returned by the native memory core. Full mode combines the fixed user and selected project; split mode shows only the selected profile category.",
    "memory.profile.priority": "Priority",
    "memory.profile.level": "Level",
    "memory.profile.stateSummary": "Profile states",
    "memory.profile.stateSummaryDescription": "Native mode shows every durable profile node. Only currently active nodes participate in profile rendering and prompt injection.",
    "memory.profile.stateActive": "Active",
    "memory.profile.statePending": "Pending review",
    "memory.profile.stateScheduled": "Scheduled",
    "memory.profile.stateInactive": "Inactive",
    "memory.profile.stateExpired": "Expired",
    "memory.profile.notInjected": "Not injected",
    "memory.profile.source": "Source",
    "memory.profile.sourceTurns": "Source turns",
    "memory.profile.statusReason": "Status reason",
    "memory.profile.updatedAt": "Updated",
    "memory.profile.storedState": "Stored state",
    "memory.profile.sourceTurnExtract": "Turn extraction",
    "memory.profile.sourceManualInstruction": "Manual profile instruction",
    "memory.profile.sourceSystemSeed": "System seed",
    "memory.profile.sourceRetained": "Retained after user deletion",
    "memory.advanced.title": "Advanced settings",
    "memory.advanced.description": "Choose the built-in native or external gRPC backend and inspect only its relevant settings and diagnostics.",
    "memory.advanced.deployment": "Deployment",
    "memory.advanced.native": "Built-in native VMM (recommended)",
    "memory.advanced.nativeDescription": "VMM Core runs directly inside Vulcan Code and reuses host AI and VLDB.",
    "memory.advanced.external": "Connect an external VMM",
    "memory.advanced.externalDescription": "Connect to a VMM service operated independently by you or your team.",
    "memory.advanced.connection": "External service connection",
    "memory.advanced.endpoint": "gRPC endpoint",
    "memory.advanced.apiKey": "Access key",
    "memory.advanced.managementEndpoint": "Management API endpoint",
    "memory.advanced.managementToken": "Management token",
    "memory.advanced.testManagement": "Save and test management connection",
    "memory.advanced.managementTimeout": "Management timeout (ms)",
    "memory.advanced.defaultBinding": "Default binding for new projects",
    "memory.advanced.nativeFixedProject": "Default fixed native memory project",
    "memory.advanced.externalPrefix": "Auto-create project prefix",
    "memory.advanced.nativeStorage": "Native storage managed by Vulcan Code",
    "memory.advanced.nativeStorageDescription": "Relational data and LanceDB vectors are stored separately under .vulcan/code/vmm. Use Memory projects for project migration and clearing.",
    "memory.advanced.runtime": "Local runtime",
    "memory.advanced.autoStart": "Start with Vulcan Code",
    "memory.advanced.manualStart": "Manual start only",
    "memory.advanced.transport": "Advanced transport",
    "memory.advanced.postactionReliability": "PostAction reliability",
    "memory.advanced.postactionTimeout": "Background analysis timeout (for example, 180s)",
    "memory.advanced.failurePassThreshold": "Consecutive-failure Pass threshold",
    "memory.advanced.maxConnectionsPerHost": "Maximum inference connections per host",
    "memory.advanced.externalReliability": "Managed by the external VMM",
    "memory.advanced.externalReliabilityDescription": "In external mode, configure the background analysis timeout and consecutive-failure Pass threshold in the external VMM configuration or environment; the external VMM owns its bounded transport connection limit. Vulcan Code does not pretend to modify those settings remotely.",
    "memory.advanced.diagnostics": "Technical diagnostics",
    "memory.advanced.diagnosticRuntime": "Runtime state",
    "memory.advanced.diagnosticBackend": "Current backend",
    "memory.advanced.diagnosticGrpc": "Service endpoint",
    "memory.advanced.diagnosticController": "Storage controller",
    "memory.advanced.diagnosticData": "Data directory",
    "memory.advanced.diagnosticLog": "Runtime log",
    "memory.advanced.data": "Memory data",
    "memory.advanced.save": "Save advanced settings",
    "memory.advanced.backupRebuild": "Back up and rebuild",
    "memory.resources.title": "Native resources and rules",
    "memory.resources.description": "Manage Prompt, PII, and Noise compositions and files. Browsing alternatives does not switch the active composition; PII and Noise include common rules.",
    "memory.resources.invalidResponse": "The native VMM resource API returned an unexpected result.",
    "memory.policy.title": "Native defaults · Recall and extraction",
    "memory.textPolicy.title": "Text intake cleanup",
    "memory.hostPolicy.title": "Native business settings for this project",
    "memory.hostPolicy.description": "Override settings for this VulcanCode project without changing memory ownership, linked reads or fixed-user profiles. Groups save independently; unmodified groups inherit current globals. Connection and injection controls remain separate.",
    "memory.hostPolicy.inherited": "Inherits current globals; edit and save to override for this project",
    "memory.hostPolicy.overridden": "Using this project's override",
    "memory.hostPolicy.inherit": "Restore global inheritance for this group",
    "memory.hostPolicy.saved": "Project settings saved for the next command.",
    "memory.hostPolicy.connection": "Customize this project",
    "memory.profilePolicy.title": "Profile lifetimes",
    "memory.profilePolicy.description": "Configure fixed-user lifetimes and project defaults independently. Applies to newly generated or revised profiles without rewriting existing deadlines.",
    "memory.profilePolicy.user": "User profiles",
    "memory.profilePolicy.project": "Project profile defaults",
    "memory.profilePolicy.transient": "Transient",
    "memory.profilePolicy.situational": "Situational",
    "memory.profilePolicy.stable": "Stable",
    "memory.profilePolicy.baseMs": "Base lifetime",
    "memory.profilePolicy.stepMs": "Extension per refresh",
    "memory.profilePolicy.maxMs": "Maximum lifetime",
    "memory.profilePolicy.unit": "Time unit (applies to all three values in this row)",
    "memory.profilePolicy.hint": "Lifetime = base + refresh weight × extension, capped at the maximum. Base must be at least 1 ms and no greater than the maximum; extension may be zero. Limit: 3650 days. Changing a unit interprets this row's values in the selected unit. Persistent profiles never expire.",
    "memory.profilePolicy.save": "Save profile lifetimes",
    "memory.profilePolicy.saved": "Profile lifetimes saved for subsequently generated or revised profiles.",
    "memory.textPolicy.description": "Applies only to newly submitted user prompts, intermediate dialogue and final replies, without reprocessing history. Media and thoughts are always removed; PII and noise follow their independent settings.",
    "memory.textPolicy.fold": "Fold long code and machine logs",
    "memory.textPolicy.budget": "Target token budget per text fragment",
    "memory.textPolicy.budgetHint": "256–131072; approximate head/tail token target, default 1200. This is not a hard limit for the whole model request. The later L1 target-plus-history budget is configured separately.",
    "memory.textPolicy.save": "Save text cleanup settings",
    "memory.textPolicy.saved": "Text cleanup settings saved for new intake fragments only.",
    "memory.policy.description": "Configure pre-turn recall and post-turn extraction independently. Explicit saves go to the user VMM directory and affect new commands only. Running commands keep their admitted policy. These global defaults do not change project bindings.",
    "memory.policy.precheckHistory": "Pre-turn recall · History turns",
    "memory.policy.precheckBudget": "Pre-turn recall · History token budget",
    "memory.policy.recallTopK": "Results recalled per query",
    "memory.policy.reviewLimit": "Merged reviewer candidate limit",
    "memory.policy.reviewScore": "Reviewer admission score floor",
    "memory.policy.queryLimit": "Retrieval query limit",
    "memory.policy.postactionHistory": "Post-turn extraction · Refined history turns",
    "memory.policy.postactionBudget": "Post-turn extraction · Target and history token budget",
    "memory.policy.extractedPriority": "New extracted memories · Default priority",
    "memory.policy.extractedWeight": "New extracted memories · Initial refresh weight",
    "memory.policy.extractedExpiry": "New extracted memories · Lifetime",
    "memory.policy.extractedStability": "New extracted memories · Retention stability",
    "memory.policy.extractedLevel": "New extracted memories · Memory level",
    "memory.policy.working": "Working",
    "memory.policy.stable": "Stable",
    "memory.policy.archival": "Archival",
    "memory.policy.expiryRange": "1 millisecond to 3650 days; must resolve to exact integer milliseconds.",
    "memory.policy.extractedHint": "These defaults affect subsequently built extraction commands only. Existing memories, profile rules and write ownership are unchanged. Lifetime starts at the original turn time, not the settings save time.",
    "memory.policy.days": "Days",
    "memory.policy.hours": "Hours",
    "memory.policy.minutes": "Minutes",
    "memory.policy.seconds": "Seconds",
    "memory.policy.milliseconds": "Milliseconds",
    "memory.policy.durationUnit": "Lifetime unit",
    "memory.policy.userSource": "Current source: User-saved global policy",
    "memory.policy.defaultSource": "Current source: Built-in defaults (no user file written)",
    "memory.policy.scoreHint": "Reviewer admission uses query-local rank projection, not raw vector or full-text scores. Token budgets govern business context, not a hard limit on the complete model request.",
    "memory.policy.invalidValues": "Enter valid values within the displayed ranges. All fields except the score must be integers.",
    "memory.policy.invalidResponse": "The native policy API did not return a valid result. The current draft has not been saved.",
    "memory.policy.saved": "Policy saved. New commands will use these settings.",
    "memory.policy.save": "Save recall and extraction policy",
    "memory.policy.resetDraft": "Fill built-in defaults (save to apply)",
    "memory.ranking.title": "Native defaults · Retrieval ranking",
    "memory.cleanup.title": "Native defaults · Periodic cleanup",
    "memory.projectRetention.title": "Project data retention",
    "memory.projectRetention.description": "Configure the memory project's hot-turn window, idle and recycle durations, and protection rules. Saved rules apply to the next cleanup; existing recycle deadlines remain unchanged. Scheduling and personal profiles remain global.",
    "memory.projectProfile.title": "Project profile rules",
    "memory.projectProfile.description": "Set initial lifetimes, refresh extensions and caps per memory project. New rules apply to new profile commands without rewriting existing nodes. Personal profiles always use global rules.",
    "memory.projectProfile.inherited": "Inheriting global project profile rules",
    "memory.projectProfile.saved": "Project profile rules saved for new profile commands.",
    "memory.projectRetention.select": "Select memory project",
    "memory.projectRetention.reload": "Reload rules",
    "memory.projectRetention.inherited": "Currently inheriting global data retention rules",
    "memory.projectRetention.overridden": "Currently using this memory project's own rules",
    "memory.projectRetention.saved": "Project retention rules saved for the next cleanup.",
    "memory.projectRetention.save": "Save project override",
    "memory.projectRetention.inherit": "Restore global inheritance",
    "memory.projectRetention.fillGlobal": "Fill current global values (not saved)",
    "memory.cleanup.description": "Tune automatic cleanup and retention. Manual runs remain available when scheduling is off. New rules apply to the next pass; existing recycle deadlines stay unchanged.",
    "memory.cleanup.enabled": "Enable periodic cleanup and profile-expiry scans",
    "memory.cleanup.scan": "Full cleanup interval (minutes)",
    "memory.cleanup.profileScan": "Profile-expiry scan interval (seconds)",
    "memory.cleanup.hotWindow": "Newest turns protected per session",
    "memory.cleanup.idle": "Session inactivity before cleanup",
    "memory.cleanup.trash": "Retention of new recycle batches",
    "memory.cleanup.protection": "Advanced · Shared-memory protection",
    "memory.cleanup.protectionHint": "When protection is enabled, shared terminal memories meeting either threshold are retained. This controls automatic recycling, not stored memory levels.",
    "memory.cleanup.protectShared": "Protect important shared terminal memories",
    "memory.cleanup.priority": "Minimum protected priority (0–100)",
    "memory.cleanup.level": "Minimum protected memory level",
    "memory.cleanup.session": "Session",
    "memory.cleanup.phase": "Phase",
    "memory.cleanup.stable": "Stable",
    "memory.cleanup.persistent": "Persistent",
    "memory.cleanup.unit": "Unit",
    "memory.cleanup.days": "days",
    "memory.cleanup.hours": "hours",
    "memory.cleanup.minutes": "minutes",
    "memory.cleanup.seconds": "seconds",
    "memory.cleanup.milliseconds": "milliseconds",
    "memory.cleanup.invalid": "Enter valid whole numbers. Intervals, retention durations, and turn counts must be positive.",
    "memory.cleanup.save": "Save cleanup settings",
    "memory.cleanup.saved": "Cleanup settings saved; workers will load the new configuration.",
    "memory.ranking.description": "Tune candidates and ranking for new searches. Saving does not regenerate vectors or re-extract old conversations. Read-time decay does not delete memories.",
    "memory.ranking.hybrid": "Vector/full-text fusion",
    "memory.ranking.mmr": "Diversity selection (MMR)",
    "memory.ranking.decay": "Read-time decay",
    "memory.ranking.advanced": "Advanced ranking parameters",
    "memory.ranking.lexical": "Shared hybrid candidate window floor",
    "memory.ranking.rrf": "Rank fusion constant (RRF)",
    "memory.ranking.replace": "Replacement reviewer score floor",
    "memory.ranking.dedupe": "Hard-dedupe cosine threshold (0 disables)",
    "memory.ranking.dedupePool": "Hard-dedupe candidate window",
    "memory.ranking.replacePool": "Replacement reviewer similar-memory count",
    "memory.ranking.rerank": "Rerank candidate count (0 disables)",
    "memory.ranking.lambda": "MMR relevance weight",
    "memory.ranking.shape": "Weibull shape",
    "memory.ranking.scale": "Weibull scale (hours)",
    "memory.ranking.floor": "Decay multiplier floor",
    "memory.ranking.reinforce": "Reinforcement weight",
    "memory.ranking.crossSession": "Cross-session adoption boost",
    "memory.ranking.scoreHint": "Vector and full-text share the hybrid candidate window. Replacement uses reviewer-projected scores; hard dedupe uses cosine independently, never raw BM25. Disabled stages retain their parameters for later re-enablement.",
    "memory.ranking.save": "Save retrieval ranking policy",
    "memory.ranking.saved": "Ranking policy saved. New commands will use these settings.",
    "memory.ranking.invalidValues": "Enter values within their valid ranges. Candidate counts must be integers; RRF must be a positive decimal integer without leading zeroes.",
    "memory.resources.loading": "Loading native resources…",
    "memory.resources.refresh": "Refresh resources",
    "memory.resources.promptBundle": "Memory prompt language",
    "memory.resources.piiLocale": "PII country/region",
    "memory.resources.noiseLocale": "Noise country/region",
    "memory.resources.noiseEnabled": "Enable Noise filtering",
    "memory.resources.noiseSemanticEnabled": "Enable semantic Noise matching",
    "memory.resources.noiseThreshold": "Default semantic Noise threshold",
    "memory.resources.saveComposition": "Save and apply now",
    "memory.resources.fileEditor": "Resource file editor",
    "memory.resources.fileEditorDescription": "Inspect and override each file independently. PII and Noise rules combine common with the selected locale.",
    "memory.resources.file": "Resource file",
    "memory.resources.userOverride": "User override",
    "memory.resources.builtIn": "Built-in source",
    "memory.resources.highRisk": "High-risk prompt modification",
    "memory.resources.promptHowTo": "Preserve structured-output constraints, field semantics, and stage boundaries. Incorrect edits can omit, miswrite, or invalidate memories.",
    "memory.resources.beginPromptEdit": "Understand risk and prepare editing",
    "memory.resources.builtInSource": "Built-in source (read-only)",
    "memory.resources.effectiveEditor": "Effective content / user override",
    "memory.resources.saveFile": "Save file",
    "memory.resources.restoreBuiltIn": "Restore built-in content",
    "memory.resources.piiTester": "PII rule tester",
    "memory.resources.piiTesterDescription": "Runs the active common-plus-locale scrubber; Core's authoritative evaluator executes every condition formula without invoking an LLM.",
    "memory.resources.piiActiveBundleTest": "Active rule-composition test",
    "memory.resources.piiFormulaTester": "Ad-hoc regex and condition tester",
    "memory.resources.piiFormulaTesterDescription": "Tests one temporary rule with Core's regex, captures, condition DSL, and replacement semantics without changing any resource file.",
    "memory.resources.piiSampleText": "Sample text",
    "memory.resources.piiPattern": "Regex pattern",
    "memory.resources.piiCondition": "Condition formula (optional)",
    "memory.resources.piiReplacement": "Replacement",
    "memory.resources.piiCandidates": "Regex candidates: {count}",
    "memory.resources.piiAccepted": "Condition accepted",
    "memory.resources.piiRejected": "Condition rejected",
    "memory.resources.piiCaptures": "Captures",
    "memory.resources.piiNoCandidates": "The regex produced no candidates.",
    "memory.resources.noiseTester": "Noise rule tester",
    "memory.resources.noiseTesterDescription": "Tests deterministic regex only; the live memory intake path owns semantic phrase matching.",
    "memory.resources.role": "Message role",
    "memory.resources.runTest": "Run test",
    "memory.resources.noMatch": "No rule matched",
    "memory.resources.noiseMatched": "Matched Noise category",
    "memory.resources.confirmPurposeTitle": "Confirm that you understand this rule file and its risks",
    "memory.resources.confirmModificationTitle": "Confirm the modification method and risk again",
    "memory.resources.confirmModificationDescription": "The existing JSON contract and field meanings must remain intact. Confirmation unlocks editing and saving; the override is written under .vulcan/code/vmm/config.",
    "memory.resources.confirmPurpose": "I understand the purpose",
    "memory.resources.unlockEditor": "I accept the risk; unlock editing",
    "memory.resources.promptPurpose.precheckL1": "PreCheck L1 makes the first memory-recall decision before a user request executes. Preserve its structured decision fields.",
    "memory.resources.promptPurpose.precheckL2": "PreCheck L2 completes second-stage recall analysis when needed. Preserve its structured decision fields.",
    "memory.resources.promptPurpose.postactionL1": "PostAction L1 extracts candidate memories after a completed turn. Changes directly affect what can enter long-term memory.",
    "memory.resources.promptPurpose.postactionL2": "PostAction L2 reviews and organizes candidates. Changes affect final writes, levels, priority, and profile nodes.",
    "memory.resources.promptPurpose.profileInstruction": "Profile instructions convert natural-language adjustments into profile-node changes. Preserve operation and target field contracts.",
    "memory.resources.promptPurpose.unknown": "This prompt belongs to Core's structured inference path. Verify its schema before editing.",
    "memory.resources.overrideDirectory": "User override directory",
    "memory.resources.promptTab": "Prompt settings",
    "memory.resources.piiTab": "PII settings",
    "memory.resources.noiseTab": "Noise settings",
    "memory.resources.unlocked": "Unlocked",
    "memory.resources.locked": "Editing locked",
    "memory.resources.guardedEdit": "Protected rule modification",
    "memory.resources.currentEffective": "Selected file content",
    "memory.resources.composition": "Active rule composition",
    "memory.resources.compositionDescription": "Configure only this category's language, locale, or policy. Saving validates and hot-reloads it immediately.",
    "memory.resources.piiPurpose": "PII rules scrub content locally before memory processing. Regex, captures, condition formulas, and replacements must remain valid.",
    "memory.resources.noisePurpose": "Noise rules decide only whether the current user prompt enters memory analysis. Common and selected country/region rules are combined.",
    "memory.resources.saveConfirmTitle": "Confirm saving this rule override again",
    "memory.resources.resetConfirmTitle": "Restore the built-in rule?",
    "memory.resources.saveConfirmDescription": "Saving writes the current content under .vulcan/code/vmm/config and hot-reloads it. Incorrect prompts or rules can omit, miswrite, or break memory processing.",
    "memory.resources.resetConfirmDescription": "This removes the user override for this file and immediately restores the embedded source.",
    "memory.resources.saveConfirmAction": "Save and apply",
    "memory.resources.resetConfirmAction": "Restore built-in content",
    "memory.resources.userPromptOnly": "User prompt only (system, assistant, and tool content are not accepted)",
    "memory.maintenance.title": "Cleanup and recycle bin",
    "memory.maintenance.description": "Periodic cleanup follows the current settings when enabled. Inspect policy, preview manual cleanup, and maintain recycled data here.",
    "memory.maintenance.invalidResponse": "The native VMM retention API returned an unexpected result.",
    "memory.maintenance.readFailed": "Could not load this status. Refresh to retry.",
    "memory.maintenance.runNow": "Run cleanup now",
    "memory.lexical.title": "Full-text search index",
    "memory.lexical.description": "Search memory summaries and details with Jieba. Recreate full-text documents from current active memories and rebuild the index without model calls or embedding generation.",
    "memory.lexical.documents": "Full-text source documents",
    "memory.lexical.rows": "Full-text index rows",
    "memory.lexical.rebuild": "Rebuild full-text index",
    "memory.lexical.confirmTitle": "Rebuild the full-text index?",
    "memory.lexical.confirmDescription": "Recreate the full-text index from {count} active memories across all native projects, repairing missing or stale derived documents. Memory content, existing vectors, project bindings, and recycle-bin contents remain unchanged. Refresh and confirm again if the content version changes.",
    "memory.lexical.completed": "Rebuilt {count} full-text rows without generating embeddings.",
    "memory.lexical.countsNote": "Counts are observational; equal counts do not prove complete tokenization. Queries still enforce project access and expiry filters.",
    "memory.maintenance.previewCleanup": "Preview cleanup",
    "memory.maintenance.cleanupPreviewTitle": "Clean up these records?",
    "memory.maintenance.cleanupPreviewDescription": "Memories and turns move to the recycle bin; profile nodes are only marked expired. Expired recycle batches and their contents are permanently removed. The preview expires after 5 minutes and must be refreshed if data or rules change. All native memory projects are included, regardless of the recycle-bin filter.",
    "memory.maintenance.cleanupImpact": "Recycle {memories} memories and {turns} turns; expire {profiles} profile nodes; permanently remove {batches} batches.",
    "memory.maintenance.cleanupRecordIds": "View affected record IDs",
    "memory.maintenance.status": "Scheduled cleanup status",
    "memory.maintenance.statusDescription": "The host serializes cleanup and commits relational data plus LanceDB vectors through one durable mutation journal.",
    "memory.maintenance.worker": "Full cleanup worker",
    "memory.maintenance.profileExpiryWorker": "Profile expiry worker",
    "memory.maintenance.running": "Running",
    "memory.maintenance.enabled": "Enabled",
    "memory.maintenance.disabled": "Disabled",
    "memory.maintenance.interval": "Interval",
    "memory.maintenance.profileExpiryInterval": "Profile expiry interval",
    "memory.maintenance.vectorTitle": "Vector generation and rebuild",
    "memory.maintenance.vectorDescription": "When the embedding model or dimension changes, active and recoverable recycle-bin memories must be explicitly rebuilt into the current generation. Previewing does not mutate data.",
    "memory.maintenance.vectorConfigured": "Current embedding route",
    "memory.maintenance.configured": "Configured",
    "memory.maintenance.notConfigured": "Unavailable",
    "memory.maintenance.vectorDimension": "Vector dimension",
    "memory.maintenance.activeMemories": "Active memories",
    "memory.maintenance.recycleBinMemories": "Recycle-bin memories",
    "memory.maintenance.currentVectors": "Active on current generation",
    "memory.maintenance.currentRecycleBinVectors": "Recycle bin on current generation",
    "memory.maintenance.pendingAnalysisJobs": "Pending analysis jobs",
    "memory.maintenance.vectorRebuildRequired": "Older vector generations were detected. Active-memory operations or restoration of an old recycle batch fail explicitly until rebuilding completes.",
    "memory.maintenance.vectorRouteUnavailable": "The embedding route cannot currently be resolved, so vector compatibility cannot yet be evaluated or rebuilt.",
    "memory.maintenance.vectorCompatible": "All active and recoverable recycle-bin memories use the vector generation for the current embedding route.",
    "memory.maintenance.vectorRebuildCounts": "Rebuild {memories} memories (active {active}, recycle bin {recycled}) across {spaces} older vector generations.",
    "memory.maintenance.previewVectorRebuild": "Preview vector rebuild",
    "memory.maintenance.confirmVectorRebuildTitle": "Rebuild native-memory vectors?",
    "memory.maintenance.confirmVectorRebuildDescription": "This rebuilds {memories} memories (active {active}, recycle bin {recycled}) across {projects} projects in {batches} bounded batches using the current embedding route at dimension {dimension}. The preview version and route are revalidated before execution.",
    "memory.maintenance.rebuildVectors": "Confirm rebuild",
    "memory.maintenance.pendingJobs": "Pending cold-turn jobs",
    "memory.maintenance.recycleBatches": "Recycle batches",
    "memory.maintenance.hotWindow": "Protected hot turns per session",
    "memory.maintenance.idleWindow": "Idle-session window",
    "memory.maintenance.trashWindow": "Recycle retention window",
    "memory.maintenance.protectionFloor": "Shared-memory protection floor",
    "memory.maintenance.limits": "Per-pass cleanup limits",
    "memory.maintenance.profileExpiryLimit": "Expired profile nodes",
    "memory.maintenance.memoryRecycleLimit": "Terminal memories",
    "memory.maintenance.coldScanLimit": "Cold-turn session scans",
    "memory.maintenance.coldJobLimit": "Cold-turn job executions",
    "memory.maintenance.idleSessionLimit": "Idle sessions",
    "memory.maintenance.trashPurgeLimit": "Expired recycle batches",
    "memory.maintenance.vectorConsistency": "Native mode does not run a separate vector GC. Active and recycled vectors are atomically reconciled by the same VLDB mutation journal and removed with permanent batch purge.",
    "memory.maintenance.lastRun": "Last completed",
    "memory.maintenance.profileExpiryLastRun": "Last profile expiry pass",
    "memory.maintenance.profileExpiryLastResult": "Expired {profiles} due profile nodes in this pass.",
    "memory.maintenance.never": "Never run",
    "memory.maintenance.lastResult": "Latest result: expired {profiles} profile nodes, recycled {memories} memories and {turns} turns, enqueued {enqueued} and completed {completed} cold-turn jobs, and permanently purged {purged} expired batches.",
    "memory.maintenance.recycleBin": "Recycle bin",
    "memory.maintenance.recycleDescription": "Original relational records and LanceDB vectors remain during recycling. Restore does not re-embed. If the embedding route changed, first run the vector rebuild above, which includes recycle-bin content. Expiry or permanent purge removes them.",
    "memory.maintenance.projectFilter": "Memory project",
    "memory.maintenance.allProjects": "All memory projects",
    "memory.maintenance.emptyRecycleBin": "No recycle batches match the current filter.",
    "memory.maintenance.confirmPurgeTitle": "Permanently purge this recycle batch?",
    "memory.maintenance.confirmRestoreTitle": "Restore this recycle batch?",
    "memory.maintenance.confirmPurgeDescription": "This permanently removes the batch's relational records and retained vectors and cannot be undone.",
    "memory.maintenance.confirmRestoreDescription": "Every durable identity is checked again before restore. Conflicts reject the operation without changing data.",
    "memory.maintenance.purge": "Permanently purge",
    "memory.maintenance.restore": "Restore",
    "memory.maintenance.turns": "Turns",
    "memory.maintenance.memories": "Memories",
    "memory.maintenance.profiles": "Profile nodes",
    "memory.maintenance.evidence": "Context evidence",
    "memory.recycleRecords.inspect": "Inspect retained content",
    "memory.recycleRecords.description": "Inspect only records retained in this batch, without restoring or merging active content. Session IDs identify record sources, not the batch's original selection.",
    "memory.recycleRecords.reload": "Reload details",
    "memory.recycleRecords.total": "{count} records",
    "memory.recycleRecords.failed": "Unable to load retained records. The batch or version may have changed. Reload and check the action notification for details.",
    "memory.recycleRecords.empty": "This batch contains no records of this type.",
    "memory.recycleRecords.previous": "Previous",
    "memory.recycleRecords.next": "Next",
    "memory.recycleRecords.session": "Source session",
    "memory.recycleRecords.savedStatus": "Extraction state at recycling",
    "memory.recycleRecords.sourceTurns": "Source turns",
    "memory.recycleRecords.userProfile": "User profile",
    "memory.recycleRecords.projectProfile": "Project profile",
    "memory.recycleRecords.support": "Support",
    "memory.recycleRecords.rebuttal": "Rebuttal",
    "memory.maintenance.expiresAt": "Automatic purge time",
    "memory.maintenance.reasonTerminal": "Terminal memory",
    "memory.maintenance.reasonColdTurn": "Cold turns",
    "memory.maintenance.reasonIdle": "Idle session",
    "memory.maintenance.reasonManualSession": "Manually recycled sessions",
    "memory.maintenance.reasonManualTurn": "Manually recycled turns",
    "memory.maintenance.manualSources": "Inspect original manual selection",
    "vmm.experience.deployment.native": "Built-in VMM",
    "vmm.experience.deployment.disabled": "Disabled",
    "vmm.experience.deployment.external": "External service",
    "vmm.experience.setup.notConfigured": "Not configured",
    "vmm.experience.setup.checking": "Checking",
    "vmm.experience.setup.ready": "Ready",
    "vmm.experience.setup.incomplete": "Needs attention",
    "vmm.experience.setup.blocked": "Blocked",
    "vmm.experience.lifecycle.stopped": "Stopped",
    "vmm.experience.lifecycle.running": "Running",
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
    "vmm.experience.prerequisite.transport.description": "Choose built-in native memory or an external service and complete the corresponding settings.",
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
    "vmm.nativeProjects.title": "Native memory projects",
    "vmm.nativeProjects.description": "Manage one-level memory projects, stable ID bindings, and recall-only linked projects. Writes always use the owner project only.",
    "vmm.nativeProjects.refresh": "Refresh catalog",
    "vmm.nativeProjects.running": "Running native memory project operation…",
    "vmm.nativeProjects.createTitle": "Create a memory project for an unbound project",
    "vmm.nativeProjects.createDescription": "The stable project tag is the default name. Creation immediately binds by ID.",
    "vmm.nativeProjects.selectVulcanProject": "Select an unbound VulcanCode project",
    "vmm.nativeProjects.projectName": "Memory project name",
    "vmm.nativeProjects.create": "Create and bind",
    "vmm.nativeProjects.bindingsTitle": "Project bindings and linked recall",
    "vmm.nativeProjects.bindingsDescription": "Each VulcanCode project has one write owner and may include multiple recall-only projects.",
    "vmm.nativeProjects.unbound": "Unbound (disable project memory)",
    "vmm.nativeProjects.saveBinding": "Save binding",
    "vmm.nativeProjects.linkedRecall": "Additional recall-only projects",
    "vmm.nativeProjects.catalogTitle": "Memory project catalog",
    "vmm.nativeProjects.catalogDescription": "Names can change without changing binding IDs. The protected default project cannot be deleted.",
    "vmm.nativeProjects.rename": "Rename",
    "vmm.nativeProjects.clear": "Clear content",
    "vmm.nativeProjects.delete": "Delete project",
    "vmm.nativeProjects.protected": "Protected",
    "vmm.nativeProjects.migrateTitle": "Migrate project memory",
    "vmm.nativeProjects.migrateDescription": "Preview source, target, post-migration totals, and session conflicts before moving content. The source project remains empty.",
    "vmm.nativeProjects.source": "Source memory project",
    "vmm.nativeProjects.target": "Target memory project",
    "vmm.nativeProjects.previewMigration": "Preview migration",
    "vmm.nativeProjects.preview.migration": "Confirm project memory migration",
    "vmm.nativeProjects.preview.clear": "Confirm project content clear",
    "vmm.nativeProjects.preview.delete": "Confirm memory project deletion",
    "vmm.nativeProjects.previewCounts": "Sessions {sessions}, memories {memories}, project profiles {profiles}, vector nodes {vectors}.",
    "vmm.nativeProjects.previewMigrationCounts": "Source now: sessions {sourceSessions}, memories {sourceMemories}, project profiles {sourceProfiles}, vector nodes {sourceVectors}; target now: sessions {targetSessions}, memories {targetMemories}, project profiles {targetProfiles}, vector nodes {targetVectors}; target after migration: sessions {afterSessions}, memories {afterMemories}, project profiles {afterProfiles}, vector nodes {afterVectors}.",
    "vmm.nativeProjects.previewConflicts": "Detected {count} session-name conflicts. Confirm to rename with -1, -2, and so on before migration.",
    "vmm.nativeProjects.previewRecords": "Review {count} primary records to migrate",
    "vmm.nativeProjects.recordKind.session": "Session",
    "vmm.nativeProjects.recordKind.turn": "Turn",
    "vmm.nativeProjects.recordKind.memory": "Memory",
    "vmm.nativeProjects.recordKind.project_profile": "Project profile",
    "vmm.nativeProjects.recordSession": "Source session: {session}",
    "vmm.nativeProjects.previewOwners": "These VulcanCode projects are bound to this memory project. Deletion detaches them and disables their VMM: {projects}",
    "vmm.nativeProjects.previewLinks": "These VulcanCode projects will lose linked recall from this memory project: {projects}",
    "vmm.nativeProjects.confirmRenameMigration": "⚠ Rename and migrate",
    "vmm.nativeProjects.confirm": "Confirm",
    "vmm.setup.title": "Quick setup for long-term memory",
    "vmm.setup.description": "Choose native memory or an external VMM and explicitly bind every currently unbound project.",
    "vmm.setup.stepLabel": "Step {current} of {total}",
    "vmm.setup.previous": "Previous",
    "vmm.setup.next": "Next",
    "vmm.setup.mode.title": "Where the memory service runs",
    "vmm.setup.mode.description": "Native mode directly uses Vulcan Code AI and storage. Choose external mode when an independent VMM service already exists.",
    "vmm.setup.native.title": "Built-in native memory",
    "vmm.setup.native.description": "The memory core runs directly inside Vulcan Code without an extra public interface.",
    "vmm.setup.external.title": "Connect to an external service",
    "vmm.setup.external.description": "Connect to a VMM run by you or your team without starting a local process.",
    "vmm.setup.inference.title": "Choose memory processing models",
    "vmm.setup.inference.description": "Select the model and account for memory extraction, organization, embeddings, and reranking. Each list is filtered to suitable models.",
    "vmm.setup.inference.required": "Configure and enable an embedding model plus the PreCheck and PostAction L1/L2 base models and accounts first. Memory is initialized only after every required route is valid; profile and reranking routes can be added later.",
    "vmm.setup.inference.openCredentials": "Configure model accounts",
    "vmm.setup.binding.title": "Project memory bindings",
    "vmm.setup.binding.description": "Choose the default for new projects, then confirm every currently unbound project. Stable IDs keep bindings intact after renames.",
    "vmm.setup.binding.fixed": "Bind to one fixed project",
    "vmm.setup.binding.fixedDescription": "New projects use one memory project; native mode defaults to the protected default project.",
    "vmm.setup.binding.separate": "Create one per project",
    "vmm.setup.binding.separateDescription": "New projects create a separate memory project from the stable project tag.",
    "vmm.setup.binding.defaultProject": "Default fixed memory project",
    "vmm.setup.binding.prefix": "Auto-create prefix (team/space)",
    "vmm.setup.binding.unboundProjects": "Currently unbound projects",
    "vmm.setup.binding.none": "There are no unbound projects.",
    "vmm.setup.binding.create": "Create and bind project",
    "vmm.setup.binding.existing": "Bind existing project",
    "vmm.setup.binding.selectProject": "Select an existing project",
    "vmm.setup.scope.title": "Choose where to enable memory",
    "vmm.setup.scope.description": "Define the memory identity, then choose where it takes effect. You can change this later under Memory rules.",
    "vmm.setup.scope.all_projects": "Enable for all projects now",
    "vmm.setup.scope.current_project": "Enable only for the current project",
    "vmm.setup.scope.currentProjectUnavailable": "No workspace project is available to link",
    "vmm.setup.scope.not_yet": "Configure only; do not enable yet",
    "vmm.setup.confirm.title": "Confirm setup",
    "vmm.setup.confirm.description": "Confirm to save the backend, default binding policy, and every explicit unbound-project decision.",
    "vmm.setup.user": "Memory user name",
    "vmm.setup.user.placeholder": "For example: Local user",
    "vmm.setup.project": "Current project name",
    "vmm.setup.project.placeholder": "For example: vulcan-code",
    "vmm.setup.safety": "Existing bindings are not rebuilt by name. Stable IDs keep them intact after project renames.",
    "vmm.setup.cancel": "Cancel",
    "vmm.setup.submit": "Configure and start",
    "vmm.setup.running": "Checking settings and creating project bindings…",
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
    "vmm.runtime.mode.external": "External VMM",
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
    "runtime.thinkingTokenCount": " ({count} tokens)",
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
    "settings.providerDescription.alibaba": "Alibaba Cloud Model Studio and Token Plan services.",
    "settings.providerDescription.openrouter": "A unified model routing platform spanning multiple models and upstream endpoints.",
    "settings.providerDescription.apikeyfun": "A third-party model relay supporting multiple compatible protocols.",
    "settings.providerDescription.claudeapi": "A third-party model relay providing Anthropic-compatible access.",
    "settings.providerDescription.qiniu": "Model inference and compatible protocol services from Qiniu Cloud.",
    "settings.providerDescription.fennoai": "A third-party model relay supporting multiple compatible protocols.",
    "settings.providerDescription.code0": "A third-party model relay supporting OpenAI- and Anthropic-compatible protocols.",
    "settings.providerDescription.commandcode": "A third-party model relay with OpenAI- and Anthropic-compatible protocols, dynamic discovery, and account usage queries.",
    "settings.providerDescription.tavily": "Web search and content extraction services designed for AI applications.",
    "settings.providerDescription.openai": "Official OpenAI models and Codex account services.",
    "settings.providerDescription.anthropic": "Official Anthropic Claude models and Claude Code account services.",
    "settings.providerDescription.google": "Official Google Gemini API and Code Assist services.",
    "settings.providerDescription.agentspace": "AgentSpace OpenAI Responses/Codex-compatible models with GPT Image 2 image generation.",
    "settings.providerDescription.tokenrhythm": "A third-party model relay with a static catalog, OpenAI Chat-compatible inference, and image generation.",
    "settings.providerName.tokenrhythm": "Token Rhythm",
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
    "settings.modelBasics": "Basic settings",
    "settings.providerRetry": "Request retry",
    "settings.providerUsage": "Usage and billing",
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
    "settings.visualLab": "Visual Lab",
    "settings.gitCommit": "Git commit",
    "settings.aboutLicenses": "About and licenses",
    "settings.archivedRestore": "Archive management",
    "settings.modelProviders.description": "Review all implemented providers and manage custom compatible providers.",
    "settings.credentialManagement.description": "Manage model accounts, protocols, usage, and default credentials.",
    "settings.modelList.description": "Review enabled models by provider and the active workbench model.",
    "settings.modelBasics.description": "Model runtime basics including context, request retry, usage, and billing.",
    "settings.providerRetry.description": "Configure retry counts, wait intervals, and backoff rules for ordinary failures and 429 rate limits.",
    "settings.providerUsage.description": "Inspect native egress usage, cost provenance, coverage, and account quota snapshots.",
    "settings.aiVendors.description": "Capability vendors and route targets shared by semantic AI tools.",
    "settings.aiWebSearch.description": "Web-search capability state and available targets.",
    "settings.aiWebExtract.description": "Web-extraction capability state and available targets.",
    "settings.aiUnderstandImage.description": "Image-understanding capability state and available targets.",
    "settings.aiUnderstandVideo.description": "Video-understanding capability entry and connection status.",
    "settings.aiUnderstandAudio.description": "Audio-understanding capability entry and connection status.",
    "settings.aiImageGeneration.description": "Image-generation capability state and available targets.",
    "settings.aiImageEdit.description": "Native image-editing models, accounts, and reviewed constraints.",
    "settings.aiVideoGeneration.description": "Video-generation capability state and available targets.",
    "settings.aiVideoEdit.description": "Native video-editing models, accounts, and reviewed constraints.",
    "settings.aiVideoExtend.description": "Native video-extension models, accounts, and duration constraints.",
    "settings.aiSpeechSynthesis.description": "Speech-synthesis capability state and available targets.",
    "settings.aiSpeechTranscribe.description": "Native transcription models, accounts, and input constraints.",
    "settings.aiMusicGeneration.description": "Music-generation capability state and available targets.",
    "settings.aiMusicCover.description": "Music-cover capability state and available targets.",
    "settings.mcpServers.description": "MCP runtime, server connections, tools, prompts, and resources.",
    "settings.runtimeDiagnostics.description": "Startup flow, runtime modules, and diagnostic logs.",
    "settings.visualLab.description": "A stable acceptance surface for unified visual tokens, ordinary controls, and product components.",
    "settings.diagnostics.logs": "Diagnostic logs",
    "settings.diagnostics.startupStatus": "Startup status",
    "settings.diagnostics.waitingStatus": "Waiting for status update",
    "settings.diagnostics.providerRequestTrace": "Provider request tracing",
    "settings.diagnostics.providerRequestTrace.completeRequests": "Grouped request and raw-response logs",
    "settings.diagnostics.providerRequestTrace.openDirectory": "Open current session trace folder",
    "settings.diagnostics.providerRequestTrace.clearDirectory": "Clear current session traces",
    "settings.diagnostics.providerRequestTrace.clearConfirm": "Delete only the diagnostic files under the current session's temp/provider-api-trace directory?",
    "settings.diagnostics.providerRequestTrace.clearSucceeded": "Provider traces for the current session were cleared.",
    "settings.diagnostics.providerRequestTrace.clearEmpty": "The current session has no Provider traces to clear.",
    "settings.diagnostics.providerRequestTrace.actionFailed": "Trace directory action failed",
    "settings.diagnostics.providerRequestTrace.description": "When enabled, the final wire request and every physical retry of one logical Provider request are recorded in one group, together with response metadata, pre-parser raw response bytes, chunk or WebSocket frame boundaries, and the failed outcome. The owning session's temp/provider-api-trace directory rotates through four logical-request slots; each attempt retains up to 64 MiB of raw response data, including bytes received before failure or manual cancellation. Files may contain prompts, tool arguments, tool results, and Provider output; enable this only while diagnosing a problem.",
    "settings.diagnostics.providerRequestTrace.disabled": "Off",
    "settings.diagnostics.providerRequestTrace.recordedData": "Recorded data",
    "settings.diagnostics.providerRequestTrace.recordedDataHelp": "Each request and its retries are grouped together, including the wire request, response metadata, raw responses before parsing, stream chunk or WebSocket frame boundaries, and failure status.",
    "settings.diagnostics.providerRequestTrace.retention": "Retention",
    "settings.diagnostics.providerRequestTrace.retentionHelp": "Each session rotates through four request groups. Each attempt retains up to 64 MiB of raw response data, including content received before failure or manual cancellation. Files are stored in the session directory:",
    "settings.diagnostics.providerRequestTrace.privacy": "Privacy",
    "settings.diagnostics.providerRequestTrace.privacyHelp": "Files may contain prompts, tool arguments, tool results, and Provider output. Enable only while diagnosing an issue.",
    "settings.diagnostics.providerRequestTrace.enabled": "Tracing",
    "settings.diagnostics.providerRequestTrace.saveFailed": "Save failed",
    "settings.diagnostics.providerRequestTrace.switch": "Record Provider requests and raw responses",
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
    "settings.customModels.enabled": "Enabled",
    "settings.customModels.protocolPreference": "Protocol preference",
    "settings.customModels.protocolPreferenceHint": "Multi-protocol providers may pin one execution protocol per model; Auto resolves by fixed priority.",
    "settings.customModels.protocolAuto": "Auto",
    "settings.customModels.protocolExplicit": "Explicit",
    "settings.customModels.protocolSelect": "Select protocol",
    "settings.customModels.saveProtocolPreference": "Save protocol preference",
    "settings.customModels.disabled": "Disabled",
    "settings.credentialModels.manage": "Manage models for this account",
    "settings.credentialModels.title": "Account models",
    "settings.credentialModels.description": "Controls only whether this account can reach each model. Conversation and internal use are controlled independently in Model List.",
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
    "settings.customProviders.protocols": "Enabled protocols",
    "settings.customProviders.protocolsHint": "Enable at least one protocol; single-protocol providers hide per-model protocol selection, while multi-protocol providers show Auto and the enabled protocols.",
    "settings.customProviders.protocolCollectionRequired": "At least one compatible protocol must be enabled.",
    "settings.customProviders.protocolCollectionDuplicate": "The protocol collection must not contain duplicates.",
    "settings.customProviders.routeDrafts": "Per-protocol route overrides",
    "settings.customProviders.routeDraftsHint": "Each enabled protocol can override its base URL, endpoint path, authentication, discovery, and fixed headers independently; empty values inherit the shared settings.",
    "settings.customProviders.routeDraftFor": "{protocol} route",
    "settings.customProviders.routeBaseUrl": "Route base URL override",
    "settings.customProviders.routeBaseUrlPlaceholder": "Leave empty to inherit the shared base URL",
    "settings.customProviders.routeEndpointPath": "Route endpoint path",
    "settings.customProviders.routeAuthentication": "Route authentication header",
    "settings.customProviders.routeDiscoveryMode": "Route model catalog",
    "settings.customProviders.routeDiscoveryPath": "Route model catalog path",
    "settings.customProviders.routeHeaders": "Fixed request headers (one key: value per line)",
    "settings.customProviders.routeInherit": "Inherit shared settings",
    "settings.customProviders.advancedOptions": "Advanced options",
    "settings.customProviders.sessionHeader": "Regular-conversation Session Header",
    "settings.customProviders.sessionHeaderHint": "Optional. Used only for regular conversations, for example x-opencode-session; conflicting existing or host-owned headers are ignored.",
    "settings.customProviders.codexCapabilities": "Codex Responses capabilities",
    "settings.customProviders.codexRemoteCompaction": "Enable remote context compaction",
    "settings.customProviders.codexRemoteCompactionHint": "Uses /responses/compact and falls back to local compaction on safe failures.",
    "settings.customProviders.codexWebsocket": "Try Responses WebSocket",
    "settings.customProviders.codexWebsocketHint": "Marks this provider as an unverified WS override and falls back to HTTP/SSE only at safe boundaries.",
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
    "settings.customModels.toolSchemaProjection": "Tool schema contract",
    "settings.customModels.toolSchemaProjectionHint": "Lightweight is the default. Use passthrough only for an explicitly verified provider contract; strict is intended for restrictive function-declaration APIs such as Gemini.",
    "settings.customModels.toolSchemaProjectionLightweight": "Lightweight (default)",
    "settings.customModels.toolSchemaProjectionPassthrough": "Passthrough",
    "settings.customModels.toolSchemaProjectionStrict": "Strict",
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
    "settings.customModels.manualDescription": "Add any real model ID manually without depending on /models discovery. The local common library is checked for a capability template first.",
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
    "settings.contextWindowPolicy.title": "Context-window policy",
    "settings.contextWindowPolicy.description": "Calculated from the raw model window: 95% effective window and 90% automatic compaction by default; empty values inherit the stronger layer.",
    "settings.contextBasics.title": "Context basics",
    "settings.mediaDelivery.title": "Media delivery limits",
    "settings.mediaDelivery.description": "Configure conversations and independent AI extensions separately. Values use KiB (1 KiB = 1024 bytes), inclusively. Zero disables Base64 for that case, not attachment storage.",
    "settings.mediaDelivery.uploadRule": "With a real upload route, files at or below its inline threshold may be inlined; otherwise they are uploaded. Without upload, a separate threshold applies. Model, protocol and upload-interface hard limits always take precedence.",
    "settings.mediaDelivery.historyRule": "These thresholds apply to new delivery intents. Model switching does not reupload historical resources or replace their original delivery identity.",
    "settings.mediaDelivery.conversation": "Conversations (including sub-agents)",
    "settings.mediaDelivery.extension": "Independent AI extensions",
    "settings.mediaDelivery.kind": "Media type",
    "settings.mediaDelivery.image": "Image",
    "settings.mediaDelivery.audio": "Audio",
    "settings.mediaDelivery.video": "Video",
    "settings.mediaDelivery.file": "File / PDF",
    "settings.mediaDelivery.withUpload": "Inline limit with upload",
    "settings.mediaDelivery.withoutUpload": "Inline limit without upload",
    "settings.mediaDelivery.aggregate": "Total Base64 budget per request",
    "settings.mediaDelivery.rawMaximum": "Backend maximum raw threshold",
    "settings.mediaDelivery.totalMaximum": "Backend maximum encoded budget",
    "settings.mediaDelivery.failureRule": "Over-limit conversation inputs remain attachments with safe addresses; independent extensions explicitly report that they cannot understand or process the file. Base64 expansion, protocol framing and context limits further constrain delivery.",
    "settings.mediaDelivery.invalidNumber": "Enter a non-negative decimal KiB value; blanks and exponential notation are not accepted.",
    "settings.mediaDelivery.invalidLimit": "An input exceeds the backend ceiling. Check the field and the limits below.",
    "settings.mediaDelivery.invalidFraction": "The KiB value must convert to whole bytes; the smallest configurable unit is one byte.",
    "settings.mediaDelivery.conflict": "Media settings changed on the server and were not overwritten. Reload before editing.",
    "settings.mediaDelivery.save": "Save media settings",
    "settings.mediaDelivery.reset": "Restore backend defaults",
    "settings.mediaDelivery.reload": "Reload and discard local edits",
    "settings.contextBasics.description": "Configure model context windows, automatic compaction, and fallback buffers while managing request retry, usage, and billing in one place.",
    "settings.contextBasics.loading": "Loading basic settings…",
    "settings.contextBasics.saveSystemDefault": "Save system default",
    "settings.contextCompactionNodes.title": "Context compaction window nodes",
    "settings.contextCompactionNodes.description": "Five fixed nodes; token values are editable, while adding, deleting, renaming, and reordering are disabled.",
    "settings.contextCompactionNodes.warning": "Changing a node value affects every session that references that node.",
    "settings.contextCompactionNodes.node1": "Node 1",
    "settings.contextCompactionNodes.node2": "Node 2",
    "settings.contextCompactionNodes.node3": "Node 3",
    "settings.contextCompactionNodes.node4": "Node 4",
    "settings.contextCompactionNodes.node5": "Node 5",
    "settings.contextCompactionNodes.invalidPositive": "Every node must be a positive safe integer.",
    "settings.contextCompactionNodes.invalidAscending": "Node token values must be strictly ascending from low to high.",
    "settings.contextCompactionNodes.save": "Save node configuration",
    "settings.contextWindowPolicy.effectivePercent": "Effective window percent",
    "settings.contextWindowPolicy.autoCompactPercent": "Automatic compaction percent",
    "settings.contextWindowPolicy.autoCompactTokenLimit": "Automatic compaction token limit",
    "settings.contextWindowPolicy.scope": "Compaction usage scope",
    "settings.contextWindowPolicy.scopeInherit": "Inherit stronger layer",
    "settings.contextWindowPolicy.scopeTotal": "Total usage",
    "settings.contextWindowPolicy.scopeBodyAfterPrefix": "Body after stable prefix",
    "settings.contextWindowPolicy.fallbackBuffer": "Fallback buffer tokens",
    "settings.contextWindowPolicy.clear": "Clear this policy layer",
    "settings.models.conversationUse": "Conversation use",
    "settings.models.multimodalUnderstandingCandidate": "Multimodal understanding candidate",
    "settings.models.multimodalUnderstandingUnavailable": "Requires image, video, or audio input plus JSON Object or JSON Schema output",
    "settings.providerTools.mediaBackendMode": "Understanding backend mode",
    "settings.providerTools.selectBackendMode": "Select an understanding backend mode",
    "settings.providerTools.providerToolMode": "Provider-specific mode",
    "settings.providerTools.modelReplacementMode": "Model replacement mode",
    "settings.providerTools.reasoningEffort": "Reasoning effort",
    "settings.providerTools.modelDefaultReasoning": "Model fixed / default",
    "settings.models.internalUse": "Plugin calls",
    "settings.models.loadingCatalog": "Loading the live model catalog…",
    "settings.models.provider": "Provider",
    "settings.models.liveModels": "Live models",
    "settings.models.noProviderSelected": "No provider selected",
    "settings.models.catalogSummary": "{provider} · {count} total",
    "settings.models.memoryOnly": "Memory only; updated at startup and after manual synchronization",
    "settings.models.emptyCatalog": "No synchronized models are available. Add an account under Model Providers and synchronize it first.",
    "settings.models.availableCredentials": "{count} available credentials",
    "settings.models.additionalConditions": "{protocol} · Additional conditions",
    "settings.models.routeEffectiveProtocol": "Backend effective protocol",
    "settings.models.routeAvailableProtocols": "Available protocols",
    "settings.models.routeBindingSource": "Binding source",
    "settings.models.routeBindingRule": "Rule ID",
    "settings.models.routeManifestRevision": "Manifest revision",
    "settings.models.routeUnresolved": "The selected account has no backend-resolved executable protocol binding; this model is blocked for that account.",
    "settings.models.routeUnavailableReason": "Backend blocking reason",
    "settings.models.routeNoRule": "default binding",
    "settings.models.blockedRouteDiagnostics": "Unavailable model routes",
    "settings.models.codexBehavior": "Codex behavior",
    "settings.models.requiredRuntimePrimitive": "Missing runtime primitive",
    "settings.models.collapseProviders": "Collapse to pill",
    "settings.models.expandProviders": "Expand provider categories",
    "settings.models.searchLabel": "Search models",
    "settings.models.searchPlaceholder": "Search model name or ID",
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
    "settings.inference.vmmReasoningNotice": "Every VMM LLM purpose only lists models that explicitly support disabled reasoning and fixes the effort to Off. Reasoning output always remains separate from the VMM JSON body.",
    "settings.inference.profileVmm": "VMM profile",
    "settings.inference.llm": "Restricted LLM",
    "settings.inference.embedding": "Embeddings",
    "settings.inference.rerank": "Rerank",
    "settings.inference.rerankEnabled": "Enable rerank",
    "settings.inference.rerankDisabled": "Optional reranking is off. Enable it, then choose a model and account.",
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
    "settings.inference.reasoningDisabled": "Off",
    "settings.inference.reasoningAdaptive": "Adaptive",
    "settings.inference.reasoningMinimal": "Minimal",
    "settings.inference.reasoningXHigh": "Extra high",
    "settings.inference.reasoningUltra": "Ultra",
    "settings.inference.embeddingTask": "Embedding task",
    "settings.inference.embeddingDimensions": "Embedding dimensions",
    "settings.inference.embeddingOutputKind": "Embedding output kind",
    "settings.inference.embeddingEncoding": "Embedding encoding",
    "settings.inference.rerankTopN": "Default result count",
    "settings.inference.rerankTruncation": "Truncation policy",
    "settings.inference.rerankReturnContent": "Return candidate content",
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
    "settings.providerTools.imageEdit": "Image editing",
    "settings.providerTools.mediaAnalyze": "Image understanding tool",
    "settings.providerTools.musicGenerate": "Music generation",
    "settings.providerTools.musicCover": "Music cover",
    "settings.providerTools.speechSynthesize": "Speech synthesis",
    "settings.providerTools.speechTranscribe": "Speech transcription",
    "settings.providerTools.videoGenerate": "Video generation",
    "settings.providerTools.videoEdit": "Video editing",
    "settings.providerTools.videoExtend": "Video extension",
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
    "settings.providerTools.systemDefaultHint": "Select an available model and account before enabling. Unavailable targets remain off; new conversations use this configuration.",
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
    "gitCommitDialog.monitoringManagerDescription": "Choose repositories to monitor and adjust their display order. Select a repository in the list to focus its actions.",
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
    "settings.aboutLicenses.description": "Application version, license entry points, and compliance notes.",
    "settings.archivedRestore.description": "View, restore or permanently delete archived conversations.",
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
    "skills.label.package": "Package",
    "skills.label.plugin": "Plugin",
    "skills.catalogSummary": "System records {system}, user records {user}, plugin records {plugin}, effective skills {effective}, catalog issues {issues}.",
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
    "workspaceToolPanel.category": "Categories",
    "skills.navigation.builtins": "Built-in plugins and management",
    "skills.navigation.externalPlugins": "Plugin list",
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
    "projectMenu.openTerminal": "Open in terminal",
    "composer.contextUsage.cacheHitRate": "Average cache hit rate",
    "composer.contextUsage.noData": "No data",
    "composer.contextUsage.usedPercent": "Context used {percent}%",
    "composer.contextUsage.totalRequests": "Total requests",
    "composer.contextUsage.averageFirstToken": "Avg first token",
    "composer.contextUsage.averageSpeed": "Avg speed",
    "composer.contextUsage.noTiming": "Paired first-token timing and output usage are unavailable",
    "composer.contextUsage.totalTokens": "Total tokens",
    "composer.capability.mediaBackendMode": "Backend mode",
    "composer.capability.mediaFixedRoute": "Fixed route",
    "composer.capability.mediaBackendNote": "Model, account, and reasoning choices apply only to this session and leave system defaults unchanged.",
    "composer.capability.mediaBackendMissing": "Configure at least one available backend mode for this media-understanding capability in Model settings first.",
    "composer.attachment.previewUnavailable": "Preview is unavailable for this file type. It can still be sent with the message.",
    "composer.attachment.stagingPending": "Staging attachments. Sending will be available when staging finishes.",
    "composer.capability.extensionHint": "Select one or more models for the AI to use as needed.",
    "composer.capability.extensionProfileMissing": "No reviewed model-extension description is available, so this model cannot be selected.",
    "composer.capability.extensionConstraints": "Constraints: {value}",
    "composer.capability.extensionLimitations": "Limitations: {value}",
    "composer.capability.mode.provider": "Provider-specific mode",
    "composer.capability.mode.model": "Model replacement mode",
    "composer.capability.reasoning": "Reasoning {effort}",
    "composer.capability.inheritGlobalReasoning": "Inherit system reasoning effort",
    "settings.usageRefresh.title": "Background account-usage refresh",
    "settings.usageRefresh.description": "Refresh only accounts that declare usage-query support and have the required credentials; the default interval is 30 minutes and the cache stays in memory.",
    "settings.usageRefresh.minute": "Every {count} minute",
    "settings.usageRefresh.minutes": "Every {count} minutes",
    "settings.usageRefresh.hour": "Every {count} hour",
    "settings.credentials.selectProvider": "Select provider",
    "settings.credentials.categoryAria": "Credential categories",
    "settings.credentials.collapse": "Collapse to pill",
    "settings.credentials.expand": "Expand credential categories",
    "settings.credentials.connecting": "Connecting to the public AI runtime",
    "settings.credentials.connectingDescription": "After startup, models are fetched live from credentials; the local model cache is not read.",
    "settings.credentials.emptyProvider": "No credential has been added for this provider",
    "settings.credentials.emptyProviderDescription": "Use the Add credential button above to create the first account for this provider.",
    "settings.credentials.emptyAll": "No model account has been added",
    "settings.credentials.emptyAllDescription": "Click Add credential to choose a provider, service entry, credential method, and protocol.",
    "settings.credentials.refreshUsage": "Refresh account usage",
    "settings.credentials.syncModels": "Sync supported models",
    "settings.credentials.enable": "Enable credential",
    "settings.credentials.disable": "Disable credential",
    "settings.credentials.disabledStatus": "Disabled",
    "settings.credentials.currentDefault": "Current default account",
    "settings.credentials.setDefault": "Set as default account",
    "settings.credentials.edit": "Edit credential",
    "settings.credentials.remove": "Delete credential",
    "settings.credentials.deleteDefaultMissingReplacement": "Choose a replacement default account before deleting the current default.",
    "settings.credentials.deleteTitle": "Delete model account",
    "settings.credentials.deleteDescription": "This will delete “{name}” and its system credential. This action cannot be undone.",
    "settings.credentials.replacementDefault": "Replacement default account",
    "settings.credentials.cancel": "Cancel",
    "settings.credentials.deleting": "Deleting…",
    "settings.credentials.confirmDelete": "Confirm delete",
    "settings.credentials.usageUnsupported": "This credential does not support account-usage lookup",
    "settings.credentials.usageNotQueried": "Usage has not been queried",
    "settings.credentials.usageUnavailable": "Account usage is temporarily unavailable",
    "settings.credentials.usageInvalid": "The account-usage response is invalid; refresh and try again",
    "settings.credentials.noQuotaWindows": "This provider does not expose quota windows",
    "settings.credentials.quotaWindow": "Quota window",
    "settings.credentials.remaining": "Remaining {value}",
    "settings.credentials.used": "Used {value}",
    "settings.credentials.notIncluded": "Not included in the current plan",
    "settings.credentials.unlimited": "Unlimited",
    "settings.credentials.unknown": "Unknown",
    "settings.credentials.connectionIncomplete": "Complete the connection fields before saving.",
    "settings.credentials.editToolTitle": "Edit tool-service credential",
    "settings.credentials.editModelTitle": "Edit model account",
    "settings.credentials.addToolTitle": "Add tool-service credential",
    "settings.credentials.addModelTitle": "Add model account",
    "settings.credentials.toolDescription": "This credential is used only for the selected provider's native AI tools; it does not create a fake conversation model.",
    "settings.credentials.modelDescription": "The protocol is bound to this account and will not switch protocol or account during model calls.",
    "settings.credentials.provider": "Provider",
    "settings.credentials.serviceEntry": "Service entry",
    "settings.credentials.autoRegion": "Region is detected automatically on save",
    "settings.credentials.credentialType": "Credential type",
    "settings.credentials.displayName": "Account display name",
    "settings.credentials.replaceToken": "Replace token (leave blank to keep it)",
    "settings.credentials.apiToken": "API token",
    "settings.credentials.managementKey": "Management key (optional)",
    "settings.credentials.managementKeyPlaceholder": "Leave blank to keep the existing management key",
    "settings.credentials.managementKeyHint": "Used only for account-usage lookup, not model calls.",
    "settings.credentials.removeManagementKey": "Remove existing management key",
    "settings.credentials.oauthClientSecretOptional": "Google OAuth client secret (optional)",
    "settings.credentials.authorizationStatus": "Authorization status",
    "settings.credentials.authorizationCode": "Authorization code",
    "settings.credentials.cancelAuthorization": "Cancel authorization",
    "settings.credentials.waitBrowser": "Waiting for browser authorization",
    "settings.credentials.saveSyncing": "Saving and syncing…",
    "settings.credentials.reauthorizeAndSave": "Reauthorize and save",
    "settings.credentials.openBrowserAuthorize": "Open browser authorization",
    "settings.credentials.saveSync": "Save and sync",
    "settings.credentials.oauth.waitingAuthorization": "Waiting for user authorization",
    "settings.credentials.oauth.waitingCallback": "Waiting for browser callback",
    "settings.credentials.oauth.finalizing": "Finalizing credential save",
    "settings.credentials.oauth.succeeded": "Authorization succeeded",
    "settings.credentials.oauth.failed": "Authorization failed",
    "settings.credentials.oauth.cancelled": "Cancelled",
    "settings.providers.managementTitle": "Provider management",
    "settings.providers.managementDescription": "Manage implemented built-in providers and custom providers that reuse existing protocol adapters in Vulcan Code.",
    "settings.providers.system": "System providers",
    "settings.providers.functional": "Functional providers",
    "settings.providers.custom": "Custom providers",
    "settings.providers.catalog": "Provider catalog",
    "settings.providers.isolatedCount": "{count} custom providers isolated",
    "settings.providers.isolatedDescription": "This provider no longer participates in runtime routing; other providers and their settings remain usable and saveable. The standalone Manifest source files below retain their original bytes on save; repair the source and restart the Service, or explicitly remove the isolated entry.",
    "settings.providers.removeIsolated": "Delete isolated entry",
    "settings.providers.diagnosticsOmitted": "Remaining diagnostics were omitted at the safety limit.",
    "settings.providers.removeIsolatedTitle": "Delete isolated custom provider",
    "settings.providers.removeIsolatedDescription": "This permanently deletes the provider's standalone Manifest, central references, connections, credentials, and model settings; other providers are unaffected.",
    "settings.providers.permanentlyDelete": "Delete permanently",
    "settings.providers.customTitle": "Custom providers",
    "settings.providers.addProvider": "Add provider",
    "settings.providers.loading": "Loading providers",
    "settings.providers.emptyCustom": "No custom providers have been added.",
    "settings.providers.table.provider": "Provider",
    "settings.providers.table.endpoint": "Endpoint",
    "settings.providers.table.resources": "Resources",
    "settings.providers.table.status": "Status",
    "settings.providers.table.priority": "Priority",
    "settings.providers.table.actions": "Actions",
    "settings.providers.table.details": "Details",
    "settings.providers.table.configurationStatus": "Configuration status",
    "settings.providers.table.operation": "Operation",
    "settings.providers.table.protocolUnavailable": "No call protocol declared",
    "settings.providers.credentialCount": "{count} credentials",
    "settings.providers.configured": "Configured",
    "settings.providers.notConfigured": "Not configured",
    "settings.providers.addCredentialFor": "Add credential for {provider}",
    "settings.providers.editFor": "Edit {provider}",
    "settings.providers.removeFor": "Remove {provider}",
    "settings.providers.priorityAria": "Provider priority",
    "settings.providers.savePriority": "Save",
  },
}
