import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { useI18n } from "../i18n";
import type { MediaAsset } from "./api";
interface Session {
  urls: string[];
  activeIndex: number;
  qrSvg: string;
  expiresAt: number;
  assetId: string | null;
}
export default function Transfer({
  asset,
  onReceived,
}: {
  asset: MediaAsset | null;
  onReceived: (asset: MediaAsset) => void;
}) {
  const { locale } = useI18n();
  const l = (zh: string, en: string) => (locale === "en" ? en : zh);
  const [session, setSession] = useState<Session | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [received, setReceived] = useState<MediaAsset | null>(null),
    [seconds, setSeconds] = useState(0);
  useEffect(() => {
    void invoke<Session | null>("transfer_status")
      .then(async (value) => {
        if (value && value.assetId !== (asset?.id ?? null)) {
          await invoke("transfer_stop");
          setSession(null);
        } else setSession(value);
      })
      .catch((e) => setError(String(e)));
    const u = listen<MediaAsset>("transfer-received", (e) => {
      setReceived(e.payload);
      onReceived(e.payload);
    });
    const ended = listen(
      "transfer-ended",
      () =>
        void invoke<Session | null>("transfer_status").then(async (value) => {
          if (value && value.assetId !== (asset?.id ?? null)) {
            await invoke("transfer_stop");
            setSession(null);
          } else setSession(value);
        }),
    );
    return () => {
      u.then((f) => f());
      ended.then((f) => f());
    };
  }, []);
  useEffect(() => {
    if (!session) return;
    const tick = () =>
      setSeconds(
        Math.max(0, Math.ceil((session.expiresAt - Date.now()) / 1000)),
      );
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [session]);
  async function start() {
    setBusy(true);
    setError("");
    try {
      setSession(
        await invoke<Session>("transfer_start", { id: asset?.id ?? null }),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function stop() {
    try {
      await invoke("transfer_stop");
      setSession(null);
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <div className="capture-transfer">
      <div className="capture-eyebrow">LOCAL TRANSFER</div>
      <h1>{l("让图片在设备间流动。", "Your images, across devices.")}</h1>
      <p>
        {l(
          "手机与电脑连接同一 Wi-Fi，扫码即可下载图片或发送照片。",
          "Connect your phone and computer to the same Wi-Fi to exchange images.",
        )}
      </p>
      <div className="capture-transfer-card">
        <div className="capture-qr">
          {session && seconds > 0 ? (
            <div dangerouslySetInnerHTML={{ __html: session.qrSvg }} />
          ) : (
            <div className="capture-qr-placeholder">▦</div>
          )}
        </div>
        <div className="capture-transfer-info">
          <small>{l("本次分享", "THIS SESSION")}</small>
          <h2>
            {asset ? asset.name : l("从手机接收图片", "Receive from phone")}
          </h2>
          <p>
            {l(
              "仅当前分享的文件可被下载。链接 10 分钟后失效，也可以随时停止。",
              "Only the selected file is shared. The link expires in 10 minutes and can be stopped at any time.",
            )}
          </p>
          {session && seconds > 0 ? (
            <>
              <div className="capture-session-status">
                <i />
                {l("连接已开启", "Session active")} · {Math.floor(seconds / 60)}
                :{String(seconds % 60).padStart(2, "0")}
              </div>
              <button
                onClick={() =>
                  api
                    .copyText(session.urls[session.activeIndex])
                    .catch((e) => setError(String(e)))
                }
              >
                {l("复制连接地址", "Copy connection link")}
              </button>
              <button className="danger" onClick={() => void stop()}>
                {l("停止分享", "Stop sharing")}
              </button>
              {session.urls.length > 1 && (
                <label>
                  {l("连接网卡", "Network address")}
                  <select
                    aria-label={l("连接网卡", "Network address")}
                    value={session.activeIndex}
                    onChange={(e) =>
                      invoke<Session>("transfer_address", {
                        index: Number(e.target.value),
                      })
                        .then(setSession)
                        .catch((e) => setError(String(e)))
                    }
                  >
                    {session.urls.map((url, i) => (
                      <option key={url} value={i}>
                        {new URL(url).host}
                      </option>
                    ))}
                  </select>
                </label>
              )}
            </>
          ) : (
            <button
              className="primary"
              disabled={busy}
              onClick={() => void start()}
            >
              {busy
                ? l("正在开启…", "Starting…")
                : l("生成二维码", "Create QR code")}
            </button>
          )}
        </div>
      </div>
      {error && (
        <div role="alert" className="capture-error">
          {error}
        </div>
      )}
      {received && (
        <div role="status" className="capture-notice">
          {l("收到手机图片，已加入历史：", "Received and added to history: ")}
          {received.width} × {received.height}
        </div>
      )}
      <div className="capture-transfer-tips">
        <span>01 · {l("连接相同 Wi-Fi", "Same Wi-Fi")}</span>
        <span>02 · {l("相机扫描二维码", "Scan with camera")}</span>
        <span>03 · {l("下载或上传图片", "Download or upload")}</span>
      </div>
      <p className="capture-footnote">
        {l(
          "首次使用请允许系统的本地网络访问。此功能通过局域网 HTTP 直传，请在可信网络使用。",
          "Allow local-network access when prompted. Transfers use HTTP on your local network; use a trusted network.",
        )}
      </p>
    </div>
  );
}
