import { valuePort, eventPort, callPort } from '../core/ports.js';
import { createNode } from '../core/node.js';

/**
 * Create an endpoint registry that maps endpoint names to port nodes.
 *
 * @param {object} deps
 * @param {function} deps.fetch - fetch implementation
 * @param {function} deps.WebSocket - WebSocket constructor
 * @param {object} deps.location - location-like object with protocol/host
 * @param {object} deps.bus - optional event bus
 */
export function createEndpointRegistry({
  fetch,
  WebSocket,
  location,
  bus,
} = {}) {
  const specs = new Map();

  function define(name, spec) {
    specs.set(name, spec);
  }

  function names() {
    return [...specs.keys()];
  }

  /* The URL a route lives at, for the callers that need the ORIGIN-level primitive rather than the
     port node: a module that owns a list and posts to it (`js/app/project-services.js`) wants
     `fetch` with `Content-Type: application/json`, and the endpoint node's POST path answers raw
     text unless the spec declares a `response`. One route table, two ways in — and a name that is
     not in the table says so, instead of fetching the current page. */
  function url(name) {
    const spec = specs.get(name);
    if (!spec) {
      throw new Error(`unknown endpoint: ${name}`);
    }
    return spec.path;
  }

  function loadManifest(json) {
    const {endpoints} = json;
    if (!endpoints || typeof endpoints !== 'object') return;
    for (const [name, spec] of Object.entries(endpoints)) {
      specs.set(name, {
        ...spec,
        method: spec.method,
        path: spec.path,
        ws: spec.ws,
        request: spec.request,
        response: spec.response,
        binary: spec.binary,
      });
    }
  }

  function endpoint(name) {
    const spec = specs.get(name);
    if (!spec) {
      throw new Error(`unknown endpoint: ${name}`);
    }
    if (spec.method === 'WS') {
      return createWsEndpoint(name, spec);
    }
    return createHttpEndpoint(name, spec);
  }

  // --- HTTP endpoint ---
  function createHttpEndpoint(id, spec) {
    const invokePort = callPort(async (args) => {
      const req = spec.request ? spec.request(args) : {};
      let url = spec.path;
      let body = undefined;
      let headers = {};

      if (spec.method === 'GET') {
        const q = req.query ?? args;
        if (q && typeof q === 'object') {
          const params = new URLSearchParams();
          for (const [k, v] of Object.entries(q)) {
            params.append(k, String(v));
          }
          const qs = params.toString();
          if (qs) url += '?' + qs;
        } else if (q != null) {
          url += '?' + String(q);
        }
      } else if (spec.method === 'POST') {
        if (req.form) {
          const fd = new FormData();
          for (const [k, v] of Object.entries(req.form)) {
            fd.append(k, v);
          }
          body = fd;
        } else {
          const b = req.body ?? args;
          if (b != null) {
            body = JSON.stringify(b);
            headers['Content-Type'] = 'application/json';
          }
        }
      }

      const res = await fetch(url, {
        method: spec.method,
        headers,
        body,
        cache: spec.method === 'GET' ? 'no-store' : undefined,
      });

      if (!res.ok) {
        const err = new Error(`HTTP ${res.status}`);
        err.status = res.status;
        throw err;
      }

      let raw;
      if (spec.binary) {
        raw = await res.blob();
      } else {
        raw = await res.text();
      }

      if (spec.response) {
        return spec.response(raw);
      }
      return raw;
    });

    return createNode({
      id,
      kind: 'endpoint',
      in: {},
      out: {},
      call: { invoke: invokePort },
    });
  }

  // --- WebSocket endpoint ---
  function createWsEndpoint(id, spec) {
    const url = (location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + spec.path;
    const messagePort = eventPort();
    const statusPort = valuePort('connecting');

    let ws = null;
    let connectResolve = null;
    let connectPromise = null;
    let sendQueue = [];
    let connecting = false;
    let closedByUser = false;

    function connect() {
      if (ws) return connectPromise;
      if (connecting) return connectPromise;
      connecting = true;
      closedByUser = false;
      statusPort.set('connecting');

      connectPromise = new Promise((resolve) => {
        connectResolve = resolve;
      });

      ws = new WebSocket(url);

      ws.onopen = () => {
        connecting = false;
        statusPort.set('open');
        connectResolve();
        // flush queue
        const queue = [...sendQueue];
        sendQueue = [];
        for (const msg of queue) {
          try { ws.send(typeof msg === 'string' ? msg : JSON.stringify(msg)); } catch {}
        }
      };

      ws.onmessage = (event) => {
        messagePort.emit(event.data);
      };

      ws.onclose = () => {
        connecting = false;
        ws = null;
        statusPort.set('closed');
        connectResolve = null;
        connectPromise = null;
      };

      ws.onerror = () => {
        // errors lead to close
      };

      return connectPromise;
    }

    const sendPort = callPort(async (payload) => {
      if (closedByUser) {
        closedByUser = false;
        await connect();
      }
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        await connect();
        if (ws.readyState !== WebSocket.OPEN) {
          sendQueue.push(payload);
          return;
        }
      }
      ws.send(typeof payload === 'string' ? payload : JSON.stringify(payload));
    });

    const closePort = callPort(async () => {
      closedByUser = true;
      if (ws) {
        ws.close();
        ws = null;
      }
      connecting = false;
      statusPort.set('closed');
    });

    // Lazy connect: connect on first message subscription
    let firstMessageSub = true;
    const originalOn = messagePort.on.bind(messagePort);
    messagePort.on = (fn) => {
      const unsub = originalOn(fn);
      if (firstMessageSub) {
        firstMessageSub = false;
        connect();
      }
      return unsub;
    };

    const node = createNode({
      id,
      kind: 'endpoint',
      in: {},
      out: {
        message: messagePort,
        status: statusPort,
      },
      call: {
        send: sendPort,
        close: closePort,
      },
      dispose: () => {
        closePort.invoke();
        messagePort.dispose?.();
      },
    });

    return node;
  }

  return { define, endpoint, url, names, loadManifest };
}