/**
 * WebSocket client — auto-reconnect + state sync.
 * Connects to /api/0/ws, deserializes ServerMessage events,
 * and dispatches to registered listeners.
 */

// WS message types — use plain strings since JSON doesn't carry enum types
export interface WsIssue {
	id: string;
	project_id: string;
	fingerprint: string;
	title: string;
	culprit: string | null;
	status: string;
	level: string;
	count: number;
	user_count: number;
	first_seen: string;
	last_seen: string;
}

export interface IssueUpdated {
	type: 'IssueUpdated';
	issue: WsIssue;
}

export interface IssueCreated {
	type: 'IssueCreated';
	issue: WsIssue;
}

export interface EventReceived {
	type: 'EventReceived';
	issue_id: string;
	event_id: string;
}

export type ServerMessage = IssueUpdated | IssueCreated | EventReceived;

type Listener = (msg: ServerMessage) => void;

class WsClient {
	private ws: WebSocket | null = null;
	private listeners: Set<Listener> = new Set();
	private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	private url: string;
	private connected = false;

	constructor(url: string) {
		this.url = url;
	}

	connect() {
		if (this.ws?.readyState === WebSocket.OPEN) return;

		this.ws = new WebSocket(this.url);

		this.ws.onopen = () => {
			this.connected = true;
			if (this.reconnectTimer) {
				clearTimeout(this.reconnectTimer);
				this.reconnectTimer = null;
			}
		};

		this.ws.onmessage = (ev) => {
			try {
				const msg = JSON.parse(ev.data) as ServerMessage;
				this.listeners.forEach((fn) => fn(msg));
			} catch {
				// ignore malformed messages
			}
		};

		this.ws.onclose = () => {
			this.connected = false;
			this.scheduleReconnect();
		};

		this.ws.onerror = () => {
			this.ws?.close();
		};
	}

	subscribe(fn: Listener): () => void {
		this.listeners.add(fn);
		return () => this.listeners.delete(fn);
	}

	isConnected(): boolean {
		return this.connected;
	}

	private scheduleReconnect() {
		if (this.reconnectTimer) return;
		this.reconnectTimer = setTimeout(() => {
			this.reconnectTimer = null;
			this.connect();
		}, 3000);
	}
}

let client: WsClient | null = null;

export function getWsClient(): WsClient {
	if (!client) {
		const proto = typeof window !== 'undefined' && window.location.protocol === 'https:' ? 'wss' : 'ws';
		const url = `${proto}://${window.location.host}/api/0/ws`;
		client = new WsClient(url);
		client.connect();
	}
	return client;
}
