import { describe, expect, it } from 'vitest';
import {
	frameLocation,
	groupFrames,
	neighbours,
	parseBreadcrumbs,
	parseContexts,
	parseExceptions,
	parseRequest,
	parseSummary,
	parseTags,
	parseUser,
	type Frame
} from './event-detail';

const frame = (inApp: boolean, over: Partial<Frame> = {}): Frame => ({
	filename: 'f',
	function: '',
	module: '',
	lineno: null,
	colno: null,
	inApp,
	preContext: [],
	contextLine: null,
	postContext: [],
	...over
});

describe('parseExceptions', () => {
	it('returns [] for missing data', () => {
		expect(parseExceptions(null)).toEqual([]);
		expect(parseExceptions({})).toEqual([]);
		expect(parseExceptions({ exception: 'nope' })).toEqual([]);
	});

	it('reverses frames so the latest call is first and tolerates partial frames', () => {
		const out = parseExceptions({
			exception: {
				values: [
					{
						type: 'TypeError',
						value: 'x is undefined',
						stacktrace: {
							frames: [{ filename: 'a.js', in_app: false }, { function: 'main', lineno: 3, in_app: true }, null, 5]
						}
					}
				]
			}
		});
		expect(out).toHaveLength(1);
		expect(out[0].type).toBe('TypeError');
		expect(out[0].frames.map((f) => f.function)).toEqual(['main', '']);
		expect(out[0].frames[0].inApp).toBe(true);
	});

	it('accepts a bare array and defaults the type', () => {
		const out = parseExceptions({ exception: [{ value: 'boom' }] });
		expect(out[0]).toMatchObject({ type: 'Error', value: 'boom', frames: [] });
	});
});

describe('frames', () => {
	it('groups consecutive library frames', () => {
		const g = groupFrames([frame(true), frame(false), frame(false), frame(true)]);
		expect(g.map((x) => [x.collapsed, x.frames.length])).toEqual([
			[false, 1],
			[true, 2],
			[false, 1]
		]);
	});
	it('never collapses a trace without in-app frames', () => {
		expect(groupFrames([frame(false), frame(false)]).every((g) => !g.collapsed)).toBe(true);
	});
	it('formats location', () => {
		expect(frameLocation(frame(true, { filename: 'a.ts', lineno: 1, colno: 2 }))).toBe('a.ts:1:2');
		expect(frameLocation(frame(true, { filename: 'a.ts', lineno: 1 }))).toBe('a.ts:1');
		expect(frameLocation(frame(true, { filename: '' }))).toBe('<unknown>');
	});
});

describe('parseBreadcrumbs', () => {
	it('handles values wrapper, numeric timestamps and missing fields', () => {
		const out = parseBreadcrumbs({
			breadcrumbs: { values: [{ timestamp: 0, category: 'ui.click', message: 'hi' }, { type: 'http' }, 'junk'] }
		});
		expect(out).toHaveLength(2);
		expect(out[0].timestamp).toBe('1970-01-01T00:00:00.000Z');
		expect(out[0].type).toBe('default');
		expect(out[1].timestamp).toBeNull();
		expect(parseBreadcrumbs({})).toEqual([]);
	});
});

describe('parseTags', () => {
	it('parses object and pair-array forms', () => {
		expect(parseTags({ tags: { a: 1, b: 'x' } })).toEqual([
			{ key: 'a', value: '1' },
			{ key: 'b', value: 'x' }
		]);
		expect(parseTags({ tags: [['a', 'b']] })).toEqual([{ key: 'a', value: 'b' }]);
		expect(parseTags({ tags: null })).toEqual([]);
	});
});

describe('request/user/contexts/summary', () => {
	it('returns null/empty when absent', () => {
		expect(parseRequest({})).toBeNull();
		expect(parseRequest({ request: {} })).toBeNull();
		expect(parseUser({})).toEqual([]);
		expect(parseContexts({ contexts: 3 })).toEqual([]);
	});
	it('parses a request', () => {
		const r = parseRequest({ request: { method: 'post', url: 'http://x/y', headers: { A: 'b' }, query_string: 'q=1' } });
		expect(r).toMatchObject({ method: 'POST', url: 'http://x/y', query: 'q=1' });
		expect(r?.headers).toEqual([{ key: 'A', value: 'b' }]);
	});
	it('parses user and contexts', () => {
		expect(parseUser({ user: { id: '1', email: '' } })).toEqual([{ key: 'id', value: '1' }]);
		const c = parseContexts({ contexts: { os: { name: 'macOS', type: 'os' }, empty: {} } });
		expect(c).toEqual([{ name: 'os', rows: [{ key: 'name', value: 'macOS' }] }]);
	});
	it('builds a summary', () => {
		expect(parseSummary({ release: '1.0', sdk: { name: 's', version: '2' } })).toEqual([
			{ key: 'release', value: '1.0' },
			{ key: 'sdk', value: 's 2' }
		]);
	});
});

describe('neighbours', () => {
	const list = [{ id: 'c' }, { id: 'b' }, { id: 'a' }];
	it('finds newer and older', () => {
		expect(neighbours(list, 'b')).toMatchObject({ index: 1, newer: { id: 'c' }, older: { id: 'a' } });
		expect(neighbours(list, 'c').newer).toBeNull();
		expect(neighbours(list, 'zz').index).toBe(-1);
	});
});
