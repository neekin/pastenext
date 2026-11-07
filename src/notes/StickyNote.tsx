import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "../i18n";
import "./notes.css";
export default function StickyNote() {
  const { t } = useI18n();
  const [text, setText] = useState(""), [loaded, setLoaded] = useState(false), [saving, setSaving] = useState(false), [error, setError] = useState("");
  const queue = useRef(Promise.resolve());
  const revision = useRef(0);
  useEffect(() => {
    void invoke<{ text: string }>("note_content").then(note => { setText(note.text); setLoaded(true); }).catch(e => setError(String(e)));
  }, []);
  return <div className="sticky-note">
    <textarea aria-label={t("notes.content")} autoFocus disabled={!loaded} value={text} placeholder={t("notes.placeholder")}
      onChange={e => {
        const next = e.target.value; setText(next); setSaving(true);
        const current = ++revision.current;
        queue.current = queue.current.then(() => invoke<void>("note_save", { text: next }))
          .then(() => { if (current === revision.current) { setSaving(false); setError(""); } })
          .catch(e => { if (current === revision.current) { setSaving(false); setError(String(e)); } });
      }} />
    <div className="sticky-status" role={error ? "alert" : "status"}>{error || t(saving ? "notes.saving" : "notes.saved")}</div>
  </div>;
}
