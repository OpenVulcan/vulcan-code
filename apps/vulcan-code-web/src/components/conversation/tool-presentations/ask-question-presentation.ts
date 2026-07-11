import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { AskQuestionPresentation, AskQuestionPresentationItem, ToolPresentationDisplayState } from "./types";
import { argumentsFromMetadata, arrayValue, compactHash, fixedHostKind, fixedHostPayload, fixedToolName, metadataValue, normalizeToolName, objectValue, stringValue } from "./presentation-value-readers";

// hydrateAskQuestionPresentation merges hidden structured ask detail into one ask presentation.
// hydrateAskQuestionPresentation 将隐藏的结构化 ask 详情合并进一条提问展示模型。
export function hydrateAskQuestionPresentation(
  presentation: AskQuestionPresentation,
  payload: ToolDetailPayload | null,
): AskQuestionPresentation {
  if (!payload) {
    return presentation;
  }
  const display = askQuestionDisplayFromMetadata(payload.metadata);
  if (!display) {
    return presentation;
  }
  return {
    ...presentation,
    ignoredAnswerText: display.ignoredAnswerText,
    questions: display.questions.length ? display.questions : presentation.questions,
    title: display.title,
  };
}

// askQuestionStatusLabel returns the compact state label shown for ask rows.
// askQuestionStatusLabel 返回提问展示行使用的紧凑状态文案。
export function askQuestionStatusLabel(state: ToolPresentationDisplayState): string {
  if (state === "running") return "Asking";
  if (state === "failed" || state === "stopped") return "Ignored";
  return "Asked";
}

// resolveAskQuestionPresentation recognizes the native ask tool and projects structured question display data.
// resolveAskQuestionPresentation 识别原生 ask 工具并投影结构化问题展示数据。
export function resolveAskQuestionPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
): AskQuestionPresentation | null {
  const toolName = fixedToolName(blocks);
  const normalizedToolName = toolName ? normalizeToolName(toolName) : null;
  if (normalizedToolName !== "ask" && fixedHostKind(blocks) !== "ask") {
    return null;
  }
  const payload = fixedHostPayload(blocks);
  const display = askQuestionDisplayFromPayload(payload);
  const fallbackQuestions = askQuestionItemsFromArguments(blocks);
  const payloadAnswers = payload ? askQuestionAnswersFromPayload(payload) : null;
  const baseQuestions = display?.questions.length ? display.questions : fallbackQuestions;
  const questions = applyAskQuestionAnswers(baseQuestions, payloadAnswers);
  if (!questions.length) {
    return null;
  }
  return {
    detailRef,
    id: askQuestionPresentationId(blocks, questions),
    ignoredAnswerText: display?.ignoredAnswerText ?? "User ignored the answer",
    kind: "ask_question",
    questions,
    state: displayState,
    title: display?.title ?? "Ask the user",
  };
}

// AskQuestionDisplayPayload stores the WebUI-only display JSON emitted by the ask tool.
// AskQuestionDisplayPayload 保存 ask 工具输出的仅供WebUI使用的展示 JSON。
interface AskQuestionDisplayPayload {
  // Display title.
  // 展示标题。
  title: string;
  // Ignored-answer placeholder.
  // 忽略回答占位文案。
  ignoredAnswerText: string;
  // Display question items.
  // 展示问题条目。
  questions: AskQuestionPresentationItem[];
}

// askQuestionDisplayFromPayload reads the fixed host_result.payload.display contract for ask.
// askQuestionDisplayFromPayload 读取 ask 固定的 host_result.payload.display 契约。
function askQuestionDisplayFromPayload(payload: Record<string, unknown> | null): AskQuestionDisplayPayload | null {
  const display = objectValue(payload?.display);
  if (!display || stringValue(display.kind) !== "ask_question") {
    return null;
  }
  const questions = arrayValue(display.questions)
    .map((item, index) => askQuestionItemFromDisplay(item, index))
    .filter((item): item is AskQuestionPresentationItem => Boolean(item));
  if (!questions.length) {
    return null;
  }
  return {
    ignoredAnswerText: stringValue(display.ignored_answer_text ?? display.ignoredAnswerText) ?? "User ignored the answer",
    questions,
    title: stringValue(display.title) ?? "Ask the user",
  };
}

// askQuestionDisplayFromMetadata reads the hidden tool-detail display contract for ask.
// askQuestionDisplayFromMetadata 读取隐藏工具详情中的 ask 展示契约。
function askQuestionDisplayFromMetadata(metadata: unknown): AskQuestionDisplayPayload | null {
  return askQuestionDisplayFromPayload(objectValue(metadataValue(metadata, ["display"])));
}

// askQuestionItemFromDisplay converts one fixed display JSON question into a render item.
// askQuestionItemFromDisplay 将固定展示 JSON 中的一条问题转换为渲染条目。
function askQuestionItemFromDisplay(value: unknown, index: number): AskQuestionPresentationItem | null {
  const item = objectValue(value);
  if (!item) {
    return null;
  }
  const header = stringValue(item.header) ?? `Question ${index + 1}`;
  const question = stringValue(item.question) ?? header;
  const answers = askQuestionAnswerList(item.answers);
  const answered = item.answered === true && answers.length > 0;
  return {
    answered,
    answers,
    header,
    id: `ask-question:${index}:${compactHash(`${header}\n${question}`)}`,
    question,
  };
}

// askQuestionItemsFromArguments reads the fixed ask arguments.questions array before completion.
// askQuestionItemsFromArguments 在完成前读取固定 ask arguments.questions 数组。
function askQuestionItemsFromArguments(blocks: ToolExecutionConversationBlock[]): AskQuestionPresentationItem[] {
  for (const block of blocks) {
    const args = objectValue(argumentsFromMetadata(block.rawMetadata));
    const rawQuestions = arrayValue(args?.questions);
    if (!rawQuestions.length) {
      continue;
    }
    const questions = rawQuestions
      .map((item, index) => askQuestionItemFromArguments(item, index))
      .filter((item): item is AskQuestionPresentationItem => Boolean(item));
    if (questions.length) {
      return questions;
    }
  }
  return [];
}

// askQuestionItemFromArguments converts one fixed ask argument question into a render item.
// askQuestionItemFromArguments 将固定 ask 参数中的一条问题转换为渲染条目。
function askQuestionItemFromArguments(value: unknown, index: number): AskQuestionPresentationItem | null {
  const item = objectValue(value);
  if (!item) {
    return null;
  }
  const header = stringValue(item.header) ?? `Question ${index + 1}`;
  const question = stringValue(item.question) ?? header;
  return {
    answered: false,
    answers: [],
    header,
    id: `ask-question:${index}:${compactHash(`${header}\n${question}`)}`,
    question,
  };
}

// askQuestionAnswersFromPayload reads the fixed ask host_result payload answers array.
// askQuestionAnswersFromPayload 读取固定 ask host_result payload 中的答案数组。
function askQuestionAnswersFromPayload(payload: Record<string, unknown>): string[][] | null {
  const answers = arrayValue(payload.answers).map(askQuestionAnswerList);
  return answers.length ? answers : null;
}

// applyAskQuestionAnswers overlays completed ask answers onto argument-derived questions.
// applyAskQuestionAnswers 将完成后的 ask 答案叠加到参数派生的问题上。
function applyAskQuestionAnswers(
  questions: AskQuestionPresentationItem[],
  answers: string[][] | null,
): AskQuestionPresentationItem[] {
  if (!answers?.length) {
    return questions;
  }
  return questions.map((question, index) => {
    const answer = answers[index] ?? [];
    return {
      ...question,
      answered: answer.length > 0,
      answers: answer,
    };
  });
}

// askQuestionAnswerList normalizes one answer array from fixed ask JSON.
// askQuestionAnswerList 规范化固定 ask JSON 中的一组答案。
function askQuestionAnswerList(value: unknown): string[] {
  return arrayValue(value)
    .map(stringValue)
    .filter((answer): answer is string => Boolean(answer));
}

// askQuestionPresentationId creates a stable expansion key for one ask display row.
// askQuestionPresentationId 为单条 ask 展示行创建稳定展开键。
function askQuestionPresentationId(blocks: ToolExecutionConversationBlock[], questions: AskQuestionPresentationItem[]): string {
  const identifier = blocks
    .map((block) => block.toolCallId ?? block.id)
    .find(Boolean);
  const questionKey = questions.map((question) => `${question.header}:${question.question}`).join("|");
  return `ask-question:${identifier ?? compactHash(questionKey)}`;
}
