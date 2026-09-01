import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { mediaDeliverySettingsFixture } from "@/test-fixtures/media-delivery";
import { MEDIA_DELIVERY_KINDS, MEDIA_DELIVERY_PURPOSES } from "./media-delivery-draft";
import { MediaDeliveryPolicyCard } from "./media-delivery-policy-card";

describe("media delivery settings card", () => {
  // SSR must expose all independent fields, exact server values and separate purpose labels without writing settings.
  // 服务端渲染必须展示全部独立字段、精确服务器值及独立用途标签，且不写入设置。
  it("renders sixteen thresholds and two budgets with explicit backend ceilings", () => {
    const settings = mediaDeliverySettingsFixture();
    settings.policy.conversation.image.without_upload_max_bytes = 16;
    settings.maxInlineRawBytes = 4096;
    settings.maxTotalEncodedBytes = 8192;
    const onSaved = vi.fn();
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><MediaDeliveryPolicyCard settings={settings} onSaved={onSaved} /></I18nProvider>);
    expect((markup.match(/<input /g) ?? [])).toHaveLength(18);
    for (const purpose of MEDIA_DELIVERY_PURPOSES) {
      for (const kind of MEDIA_DELIVERY_KINDS) {
        for (const field of ["with_upload_max_bytes", "without_upload_max_bytes"]) {
          expect(markup).toContain(`id="${purpose}-${kind}-${field}"`);
        }
      }
      expect(markup).toContain(`id="${purpose}-max_total_encoded_bytes"`);
    }
    expect(markup).toContain('value="0.015625"');
    expect(markup).toContain("主会话（含子智能体会话）");
    expect(markup).toContain("独立 AI 扩展");
    expect(markup).toContain("4 KiB");
    expect(markup).toContain("8 KiB");
    expect(markup).toContain("不影响附件保存");
    expect(markup).toContain("恢复后端默认值");
    expect(onSaved).not.toHaveBeenCalled();
  });

  // English uses the same form contract and explains history and upload limits without untranslated keys.
  // 英文使用相同表单契约，并说明历史及上传限制，不显示未翻译键。
  it("renders localized English labels and safety explanations", () => {
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><MediaDeliveryPolicyCard settings={mediaDeliverySettingsFixture()} onSaved={() => undefined} /></I18nProvider>);
    expect(markup).toContain("Media delivery limits");
    expect(markup).toContain("Independent AI extensions");
    expect(markup).toContain("File / PDF");
    expect(markup).toContain("original delivery identity");
    expect(markup).not.toContain("settings.mediaDelivery.");
  });
});
