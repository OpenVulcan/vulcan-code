import React from "react";
import type { AnswerComponent } from "./answer-component-model";

type SequenceComponent = Extract<AnswerComponent, { kind: "sequence" }>;

/** Actors on top, lifelines below, one row per message, note or phase, framed like the other diagrams.
 * 参与者在顶部，生命线在下方，每条消息、注释或阶段占一行，与其他图表使用同一外框。 */
export function AnswerSequenceView({ sequence }: { sequence: SequenceComponent }) {
  let number = 0;
  return (
    <figure className="vk-figure vk-a-sequence-frame">
      <div className="vk-a-sequence" style={{ "--vk-a-actors": sequence.actors.length } as React.CSSProperties}>
        {sequence.actors.map((actor, index) => <span className="vk-a-sequence-actor" key={index}>{actor}</span>)}
        {sequence.steps.map((step, index) => {
          if (step.kind === "phase") {
            return <div className="vk-a-sequence-phase" key={index}><span>{step.text}</span></div>;
          }
          if (step.kind === "note") {
            return (
              <div className="vk-a-sequence-note" data-single={step.first === step.last || undefined} key={index}
                style={{ gridColumn: `${step.first + 1} / ${step.last + 2}`, "--vk-a-span": step.last - step.first + 1 } as React.CSSProperties}>
                {step.text}
              </div>
            );
          }
          number += 1;
          const first = Math.min(step.from, step.to);
          const last = Math.max(step.from, step.to);
          const text = sequence.numbered ? `${number}. ${step.text}` : step.text;
          return (
            <div className="vk-a-sequence-message" data-dashed={step.dashed || undefined} data-mark={step.mark} key={index}
              data-direction={step.from === step.to ? "self" : step.to < step.from ? "left" : "right"}
              style={{ gridColumn: `${first + 1} / ${last + 2}`, "--vk-a-span": last - first + 1 } as React.CSSProperties}>
              <span>{text || " "}</span>
            </div>
          );
        })}
      </div>
    </figure>
  );
}
