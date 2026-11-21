import * as React from "react";
import { formatMessage, useI18n } from "@/i18n";
import { loadPublicAiSettings, saveEconomyWindow } from "@/service/settings-actions-service";
import type { PublicAiSettingsSnapshot } from "@/types";
import { Button } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";

/** Edits the two thresholds as one application setting and previews the next batch.
 * 将两个阈值作为一项应用配置编辑，并预览下次裁剪。 */
export function EconomySettingsArea() {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot>();
  const [trigger, setTrigger] = React.useState("");
  const [batch, setBatch] = React.useState("");
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState("");
  /** Shows a saved window or factory defaults without implicitly writing either value.
   * 显示已保存或出厂轮数，不隐式写入配置。 */
  function fill(window: PublicAiSettingsSnapshot["economyWindow"]) {
    setTrigger(String(window.triggerTurns));
    setBatch(String(window.batchTurns));
  }
  React.useEffect(() => {
    let active = true;
    void loadPublicAiSettings().then((next) => {
      if (active) { setSnapshot(next); fill(next.economyWindow); }
    }).catch((reason: unknown) => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, []);
  const triggerTurns = Number(trigger);
  const batchTurns = Number(batch);
  const valid = [triggerTurns, batchTurns].every((n) => Number.isSafeInteger(n) && n > 0 && n <= 0xffffffff) && batchTurns < triggerTurns;
  const dirty = snapshot && (triggerTurns !== snapshot.economyWindow.triggerTurns || batchTurns !== snapshot.economyWindow.batchTurns);
  /** Persists one valid pair and adopts the snapshot returned by Core.
   * 保存一组有效阈值，并采用 Core 返回的快照。 */
  async function save() {
    if (!valid || !dirty || saving) return;
    setSaving(true);
    setError("");
    try {
      const next = await saveEconomyWindow({ triggerTurns, batchTurns });
      setSnapshot(next);
      fill(next.economyWindow);
    } catch (reason) { setError(String(reason)); }
    finally { setSaving(false); }
  }
  return <section className="context-basics-section">
    <header className="context-basics-head"><div><h3>{t("economy.title")}</h3><p>{t("economy.scope")}</p></div></header>
    {error && <p role="alert" className="vk-field-error">{error}</p>}
    {snapshot ? <fieldset disabled={saving} className="vk-form-prefs context-basics-rows">
      <label className="vk-form-row"><span>{t("economy.trigger")}</span><Input type="number" min={2} step={1} value={trigger} onChange={(event) => setTrigger(event.target.value)} /></label>
      <label className="vk-form-row"><span>{t("economy.batch")}</span><Input type="number" min={1} step={1} value={batch} onChange={(event) => setBatch(event.target.value)} /></label>
      {valid ? <p className="vk-form-note">{formatMessage(t("economy.preview"), { keep: triggerTurns - batchTurns, batch: batchTurns })}</p> : <p className="vk-field-error" role="alert">{t("economy.invalid")}</p>}
      <div className="vk-form-actions">
        <Button size="sm" variant="ghost" onClick={() => fill(snapshot.defaultEconomyWindow)}>{t("projectIcon.resetDefault")}</Button>
        <Button size="sm" disabled={!dirty || !valid} onClick={() => void save()}>{t("settings.misc.save")}</Button>
      </div>
    </fieldset> : !error && <p>{t("settings.contextBasics.loading")}</p>}
  </section>;
}
