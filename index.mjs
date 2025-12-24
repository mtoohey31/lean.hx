// @ts-check
import { loadRenderInfoview } from "/infoview/loader.production.min.js";

const resp = await fetch("/wsport");
if (!resp.ok) throw new Error(`wsport failed with status: ${resp.status}`);

const url = new URL(window.location.href);
url.pathname = "";
url.port = await resp.text();
const ws = new WebSocket(url);

/**
 * @typedef {import("./vendor/vscode-lean4/lean4-infoview-api/dist/infoviewApi.d.ts").EditorApi} IEditorApi
 * @implements IEditorApi
 */
class EditorApi {
  /**
   * @param {import("./vendor/vscode-lean4/lean4-infoview-api/dist/infoviewApi.d.ts").InfoviewConfig} _config
   * @return {Promise<any>}
   */
  async saveConfig(_config) {
    console.warn("saveConfig unimplemented");
  }

  /**
   * @param {string} uri
   * @param {string} method
   * @param {any} params
   * @return {Promise<any>}
   */
  async sendClientRequest(uri, method, params) {
    const urlParams = new URLSearchParams([
      ["uri", uri],
      ["method", method],
    ]);
    const resp = await fetch(`/sendclientrequest?${urlParams}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(params),
    });
    if (!resp.ok)
      throw new Error(`sendclientrequest failed with status: ${resp.status}`);
    return await resp.json();
  }

  /**
   * @param {string} uri
   * @param {string} method
   * @param {any} params
   * @return {Promise<void>}
   */
  async sendClientNotification(uri, method, params) {
    const urlParams = new URLSearchParams([
      ["uri", uri],
      ["method", method],
    ]);
    const resp = await fetch(`/sendclientnotification?${urlParams}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(params),
    });
    if (!resp.ok)
      throw new Error(
        `sendclientnotification failed with status: ${resp.status}`,
      );
  }

  /**
   * @param {string} method
   * @return {Promise<void>}
   */
  async subscribeServerNotifications(method) {
    const params = new URLSearchParams([["method", method]]);
    const resp = await fetch(`/subscribeservernotifications?${params}`, {
      method: "POST",
    });
    if (!resp.ok) {
      console.error(resp, await resp.text());
      throw new Error(
        `subscribeservernotifications failed with status: ${resp.status}`,
      );
    }
  }

  /**
   * @param {string} method
   * @return {Promise<void>}
   */
  async unsubscribeServerNotifications(method) {
    const params = new URLSearchParams([["method", method]]);
    const resp = await fetch(`/unsubscribeservernotifications?${params}`, {
      method: "DELETE",
    });
    if (!resp.ok)
      throw new Error(
        `unsubscribeservernotifications failed with status: ${resp.status}`,
      );
  }

  /**
   * @param {string} method
   * @return {Promise<void>}
   */
  async subscribeClientNotifications(method) {
    console.warn(
      `subscribeClientNotifications unimplemented (method: ${method})`,
    );
  }

  /**
   * @param {string} method
   * @return {Promise<void>}
   */
  async unsubscribeClientNotifications(method) {
    console.warn(
      `unsubscribeClientNotifications unimplemented (method: ${method})`,
    );
  }

  /**
   * @param {string} _text
   * @return {Promise<void>}
   */
  async copyToClipboard(_text) {
    console.warn("copyToClipboard unimplemented");
  }

  /**
   * @param {string} _text
   * @param {import("./vendor/vscode-lean4/lean4-infoview-api/dist/infoviewApi.d.ts").TextInsertKind} _kind
   * @param {import("./vendor/vscode-lean4/node_modules/vscode-languageserver-protocol/lib/common/protocol.d.ts").TextDocumentPositionParams} _pos
   * @return {Promise<void>}
   */
  async insertText(_text, _kind, _pos) {
    console.warn("insertText unimplemented");
  }

  /**
   * @param {import("./vendor/vscode-lean4/node_modules/vscode-languageserver-types/lib/esm/main.d.ts").WorkspaceEdit} _te
   * @return {Promise<void>}
   */
  async applyEdit(_te) {
    console.warn("applyEdit unimplemented");
  }

  /**
   * @param {import("./vendor/vscode-lean4/node_modules/vscode-languageserver-protocol/lib/common/protocol.d.ts").ShowDocumentParams} _show
   * @return {Promise<void>}
   */
  async showDocument(_show) {
    console.warn("showDocument unimplemented");
  }

  /**
   * @param {string} _uri
   * @return {Promise<void>}
   */
  async restartFile(_uri) {
    console.warn("restartFile unimplemented");
  }

  /**
   * @param {import("./vendor/vscode-lean4/node_modules/vscode-languageserver-types/lib/esm/main.d.ts").DocumentUri} uri
   * @return {Promise<string>}
   */
  async createRpcSession(uri) {
    const params = new URLSearchParams();
    if (uri !== undefined) params.append("uri", uri);
    const resp = await fetch(`/createrpcsession?${params}`, {
      method: "POST",
    });
    if (!resp.ok)
      throw new Error(`createrpcsession failed with status: ${resp.status}`);
    return resp.text();
  }

  /**
   * @param {string} sessionId
   * @return {Promise<void>}
   */
  async closeRpcSession(sessionId) {
    const params = new URLSearchParams([["session_id", sessionId]]);
    const resp = await fetch(`/closerpcsession?${params}`, {
      method: "DELETE",
    });
    if (!resp.ok)
      throw new Error(`closerpcsession failed with status: ${resp.status}`);
  }
}

ws.addEventListener("error", console.error);

ws.addEventListener("close", (_ev) => {
  window.close();
});

// Need to ensure the socket is connected before any subscriptions can be added
// otherwise messages may be missed.
ws.addEventListener("open", (_ev) => {
  loadRenderInfoview(
    {
      "@leanprover/infoview": "/infoview/index.production.min.js",
      react: "/infoview/react.production.min.js",
      "react/jsx-runtime": "/infoview/react-jsx-runtime.production.min.js",
      "react-dom": "/infoview/react-dom.production.min.js",
    },
    [new EditorApi(), document.getElementById("infoview-root")],
    /**
     * @param {import("./vendor/vscode-lean4/lean4-infoview-api/dist/infoviewApi.d.ts").InfoviewApi} infoviewApi
     */
    async (infoviewApi) => {
      ws.addEventListener("message", async (ev) => {
        /**
         * @typedef {{type: "got_server_notification", method: string, params: any}} GotServerNotification
         * @typedef {{type: "sent_client_notification", method: string, params: any}} SentClientNotification
         * @typedef {{type: "changed_cursor_location", loc: import("./vendor/vscode-lean4/node_modules/vscode-languageserver-types/lib/esm/main.d.ts").Location}} ChangedCursorLocation
         */

        /** @type {GotServerNotification | SentClientNotification | ChangedCursorLocation} */
        const s = JSON.parse(ev.data);
        switch (s.type) {
          case "got_server_notification":
            await infoviewApi.gotServerNotification(s.method, s.params);
            break;

          case "sent_client_notification":
            await infoviewApi.sentClientNotification(s.method, s.params);
            break;

          case "changed_cursor_location":
            await infoviewApi.changedCursorLocation(s.loc);
            break;
        }
      });

      const resp = await fetch("/initialize");
      if (!resp.ok)
        throw new Error(`initialize failed with status: ${resp.status}`);

      const { loc, initialize_result } = await resp.json();
      await infoviewApi.initialize(loc);
      await infoviewApi.serverRestarted(initialize_result);
    },
  );
});
