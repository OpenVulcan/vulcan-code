import React from "react";
import type { AnswerComponent, AnswerFlowChain, AnswerFlowNode } from "./answer-component-model";

type FlowComponent = Extract<AnswerComponent, { kind: "flow" }>;
type SequenceComponent = Extract<AnswerComponent, { kind: "sequence" }>;

/** Lay a flow out flat: each source line is one chain, in reading order, wrapping with the reply.
 * 平铺流程：每行源码为一条链，按阅读顺序排列，并随回答宽度换行。 */
export function AnswerFlowView({ flow }: { flow: FlowComponent }) {
  return (
    <div className="answer-flow" data-direction={flow.horizontal ? "horizontal" : "vertical"}>
      {flow.chains.map((chain, index) => <FlowChain chain={chain} key={index} nodes={flow.nodes} />)}
    </div>
  );
}

// One chain: node groups (a group fans out to several nodes) joined by arrows.
// 单条链：由箭头连接的节点组（一组可扇出为多个节点）。
function FlowChain({ chain, nodes }: { chain: AnswerFlowChain; nodes: AnswerFlowNode[] }) {
  return (
    <div className="answer-flow-chain" data-mark={chain.mark}>
      {chain.groups.map((group, index) => (
        <React.Fragment key={index}>
          {index > 0 ? (
            <span className="answer-flow-arrow" data-dashed={chain.dashed[index - 1] || undefined}>
              {chain.label && index === chain.groups.length - 1 ? <em>{chain.label}</em> : null}
            </span>
          ) : null}
          <span className="answer-flow-group">
            {group.map((id) => (
              <span className="answer-flow-node" data-mark={nodes[id].mark} data-shape={nodes[id].shape} key={id}>{nodes[id].label}</span>
            ))}
          </span>
        </React.Fragment>
      ))}
    </div>
  );
}

/** Actors on top, lifelines below, one row per message, note or phase.
 * 参与者在顶部，生命线在下方，每条消息、注释或阶段占一行。 */
export function AnswerSequenceView({ sequence }: { sequence: SequenceComponent }) {
  let number = 0;
  return (
    <div className="answer-sequence" style={{ "--answer-actors": sequence.actors.length } as React.CSSProperties}>
      {sequence.actors.map((actor, index) => <span className="answer-sequence-actor" key={index}>{actor}</span>)}
      {sequence.steps.map((step, index) => {
        if (step.kind === "phase") {
          return <div className="answer-sequence-phase" key={index}><span>{step.text}</span></div>;
        }
        if (step.kind === "note") {
          return (
            <div className="answer-sequence-note" key={index} style={{ gridColumn: `${step.first + 1} / ${step.last + 2}` }}>
              {step.text}
            </div>
          );
        }
        number += 1;
        const first = Math.min(step.from, step.to);
        const last = Math.max(step.from, step.to);
        const text = sequence.numbered ? `${number}. ${step.text}` : step.text;
        return (
          <div className="answer-sequence-message" data-dashed={step.dashed || undefined} data-mark={step.mark} key={index}
            data-direction={step.from === step.to ? "self" : step.to < step.from ? "left" : "right"}
            style={{ gridColumn: `${first + 1} / ${last + 2}`, "--answer-span": last - first + 1 } as React.CSSProperties}>
            <span>{text || " "}</span>
          </div>
        );
      })}
    </div>
  );
}
