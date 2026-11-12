import { spawn } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { once } from 'node:events';
import { preview } from 'vite';

export async function withPreview(outDir, task) {
    const server = await preview({ build: { outDir }, preview: { host: '127.0.0.1' } });
    const { port } = server.httpServer.address();
    try {
        return await task(`http://127.0.0.1:${port}/`);
    } finally {
        await new Promise((resolve, reject) => server.httpServer.close(error => error ? reject(error) : resolve()));
    }
}

export async function withBrowser(executable, task) {
    const temporaryDirectory = resolve(tmpdir());
    const profile = await mkdtemp(join(temporaryDirectory, 'graphics-chrome-'));
    const browser = spawn(executable, [
        '--headless=new', '--remote-debugging-pipe', '--no-first-run',
        '--disable-background-timer-throttling', '--disable-renderer-backgrounding',
        '--window-size=1200,900', `--user-data-dir=${profile}`,
    ], { stdio: ['ignore', 'ignore', 'ignore', 'pipe', 'pipe'], windowsHide: true });
    const exited = once(browser, 'exit').catch(() => {});
    const pending = new Map();
    let nextId = 0;
    let buffer = '';
    browser.stdio[4].setEncoding('utf8');
    browser.stdio[4].on('data', chunk => {
        buffer += chunk;
        let end;
        while ((end = buffer.indexOf('\0')) !== -1) {
            const message = JSON.parse(buffer.slice(0, end));
            buffer = buffer.slice(end + 1);
            const request = pending.get(message.id);
            if (!request) continue;
            pending.delete(message.id);
            if (message.error) request.reject(new Error(message.error.message));
            else request.resolve(message.result);
        }
    });
    const rejectPending = error => {
        for (const request of pending.values()) request.reject(error);
        pending.clear();
    };
    browser.on('error', rejectPending);
    browser.on('exit', () => rejectPending(new Error('Chromium exited')));
    browser.stdio[3].on('error', rejectPending);

    function send(method, params = {}, sessionId) {
        return new Promise((resolve, reject) => {
            const id = ++nextId;
            const timer = setTimeout(() => {
                pending.delete(id);
                reject(new Error(`Chromium did not respond to ${method}`));
            }, 180_000);
            pending.set(id, {
                resolve: value => { clearTimeout(timer); resolve(value); },
                reject: error => { clearTimeout(timer); reject(error); },
            });
            browser.stdio[3].write(JSON.stringify({ id, method, params, sessionId }) + '\0');
        });
    }

    try {
        const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
        const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
        const page = (method, params) => send(method, params, sessionId);
        const evaluate = async (expression) => {
            const { result, exceptionDetails } = await page('Runtime.evaluate', {
                expression, awaitPromise: true, returnByValue: true,
            });
            if (exceptionDetails) throw new Error(result.description || exceptionDetails.text);
            return result.value;
        };
        return await task({ page, evaluate, version: await send('Browser.getVersion') });
    } finally {
        if (browser.pid && browser.exitCode === null) {
            const forceClose = setTimeout(() => browser.kill(), 3000);
            await Promise.race([send('Browser.close').catch(() => {}), exited]);
            await exited;
            clearTimeout(forceClose);
        }
        if (dirname(resolve(profile)) === temporaryDirectory) {
            await rm(profile, { recursive: true, force: true, maxRetries: 5 }).catch(error => {
                console.warn(`Could not remove browser profile ${profile}: ${error.message}`);
            });
        }
    }
}

export async function until(check, timeoutMs = 20_000) {
    const deadline = Date.now() + timeoutMs;
    do {
        const result = await check();
        if (result) return result;
        await new Promise(resolve => setTimeout(resolve, 100));
    } while (Date.now() < deadline);
    throw new Error('Browser condition timed out');
}
