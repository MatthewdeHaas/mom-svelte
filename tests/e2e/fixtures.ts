import { test as base, type Page } from '@playwright/test';
import { readFileSync } from 'fs';
import { join } from 'path';

interface AuthState {
  adminToken: string;
  clientToken: string;
  pendingToken: string;
  companyId: string;
  menuId: string;
  menuRecipeId: string;
  tomorrowStr: string;
}

function loadState(): AuthState {
  return JSON.parse(
    readFileSync(join(process.cwd(), 'tests/e2e/.auth/state.json'), 'utf-8')
  );
}

/**
 * Inject a Better Auth session token as a cookie so the app treats the browser
 * as the given user — no actual login flow required.
 */
async function injectSession(page: Page, token: string) {
  await page.context().addCookies([
    {
      name: 'better-auth.session_token',
      value: token,
      domain: 'localhost',
      path: '/',
      httpOnly: true,
      secure: false,
      sameSite: 'Lax',
    },
  ]);
}

/**
 * Prevent Vite dev-server full-reload events from triggering page reloads.
 *
 * In Vite dev mode the dependency optimizer discovers new packages when a
 * route is first visited and broadcasts a `full-reload` event to all connected
 * WebSocket clients. The `@vite/client` script handles this by calling
 * `location.reload()`. During Playwright tests this reload races with
 * SvelteKit's client-side navigation (use:enhance fetch, goto, etc.) and
 * cancels in-progress requests.
 *
 * We suppress it by wrapping the native WebSocket so any server message
 * containing `"type":"full-reload"` is silently dropped before it reaches
 * the Vite HMR client handler. All other messages are forwarded normally.
 *
 * This runs as an init script (before any page scripts) via addInitScript,
 * so the WebSocket override is in place before @vite/client opens its socket.
 */
async function suppressViteFullReload(page: Page) {
  await page.addInitScript(() => {
    const NativeWebSocket = window.WebSocket;

    // @ts-ignore
    window.WebSocket = function (url: string, protocols?: string | string[]) {
      const ws = new NativeWebSocket(url, protocols);

      const originalAddEventListener = ws.addEventListener.bind(ws);

      // Intercept addEventListener('message', ...) calls
      ws.addEventListener = function (
        type: string,
        listener: EventListenerOrEventListenerObject,
        options?: boolean | AddEventListenerOptions
      ) {
        if (type === 'message') {
          const wrappedListener = (event: Event) => {
            const messageEvent = event as MessageEvent;
            try {
              const data = JSON.parse(messageEvent.data);
              if (data?.type === 'full-reload' || data?.type === 'update') return; // drop HMR
            } catch {
              // not JSON — forward
            }
            if (typeof listener === 'function') {
              listener.call(ws, messageEvent);
            } else {
              listener.handleEvent(messageEvent);
            }
          };
          return originalAddEventListener(type, wrappedListener, options);
        }
        return originalAddEventListener(type, listener, options);
      };

      // Also intercept the onmessage property setter
      let _onmessage: ((this: WebSocket, ev: MessageEvent) => void) | null = null;
      Object.defineProperty(ws, 'onmessage', {
        get() { return _onmessage; },
        set(handler: ((this: WebSocket, ev: MessageEvent) => void) | null) {
          _onmessage = handler;
          if (handler) {
            originalAddEventListener('message', (event: Event) => {
              const messageEvent = event as MessageEvent;
              try {
                const data = JSON.parse(messageEvent.data);
                if (data?.type === 'full-reload' || data?.type === 'update') return;
              } catch { /* not JSON */ }
              handler.call(ws, messageEvent);
            });
          }
        },
        configurable: true,
      });

      return ws;
    };

    // Copy static properties from native WebSocket
    window.WebSocket.prototype = NativeWebSocket.prototype;
    (window.WebSocket as any).CONNECTING = NativeWebSocket.CONNECTING;
    (window.WebSocket as any).OPEN = NativeWebSocket.OPEN;
    (window.WebSocket as any).CLOSING = NativeWebSocket.CLOSING;
    (window.WebSocket as any).CLOSED = NativeWebSocket.CLOSED;
  });
}

type Fixtures = {
  adminPage: Page;
  clientPage: Page;
  pendingPage: Page;
  authState: AuthState;
};

export const test = base.extend<Fixtures>({
  authState: async ({}, use) => {
    await use(loadState());
  },

  adminPage: async ({ page }, use) => {
    await suppressViteFullReload(page);
    await injectSession(page, loadState().adminToken);
    await use(page);
  },

  clientPage: async ({ page }, use) => {
    await suppressViteFullReload(page);
    await injectSession(page, loadState().clientToken);
    await use(page);
  },

  pendingPage: async ({ page }, use) => {
    await suppressViteFullReload(page);
    await injectSession(page, loadState().pendingToken);
    await use(page);
  },
});

export { expect } from '@playwright/test';
