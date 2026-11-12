import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import assert from 'node:assert/strict';
import { until, withBrowser, withPreview } from './browser.mjs';

const [browser, url, output, mode] = process.argv.slice(2);
if (!browser || !url || !output) throw new Error('Usage: node scripts/smoke.mjs CHROME URL OUTPUT_DIRECTORY');
await mkdir(output, { recursive: true });

const smoke = (origin) => withBrowser(browser, async ({ page, evaluate, version }) => {
    await page('Page.enable');
    if (mode === '--unsupported') {
        await page('Page.addScriptToEvaluateOnNewDocument', {
            source: "Object.defineProperty(globalThis, 'SharedArrayBuffer', { value: undefined })",
        });
    }
    const query = mode === '--benchmark-preview' ? '?benchmark=C'
        : mode === '--walkthrough' ? '?threads=1' : '?threads=8';
    const navigation = await page('Page.navigate', { url: new URL(query, origin).href });
    if (navigation.errorText) throw new Error(navigation.errorText);
    if (mode === '--unsupported') {
        const message = await until(() => evaluate("document.querySelector('[role=alert]')?.textContent"));
        assert.match(message, /Renderer unavailable.*SharedArrayBuffer.*cross-origin isolation/);
        console.log(message);
        return;
    }
    if (mode === '--benchmark-preview') {
        const runtime = await until(() => evaluate('window.graphicsBenchmark?.runtime'));
        assert.equal(runtime.actualThreads, 8);
        await evaluate(`void (window.cancellationCheck = window.graphicsBenchmark.run({
            full: false, width: 96, height: 72, samples: 4096, depth: 5, seed: 20261112, budgetMs: 90_000,
        }))`);
        try {
            await until(() => evaluate("document.querySelector('progress')?.value > 0"), 5_000);
        } catch (error) {
            console.error(await evaluate("({ text: document.body.innerText, progress: document.querySelector('progress')?.outerHTML })"));
            throw error;
        }
        await evaluate("[...document.querySelectorAll('button')].find(button => button.textContent === 'Cancel benchmark').click()");
        const cancelled = await evaluate('window.cancellationCheck');
        assert.equal(cancelled.record.status, 'cancelled');
        assert.equal(cancelled.png, null);
        assert.equal(cancelled.linear, null);
        const status = await until(() => evaluate("document.querySelector('[role=status]')?.textContent"));
        assert.match(status, /cancelled.*\/4096 spp/);
        assert.equal(await evaluate("Boolean(document.querySelector('a[download]'))"), false);
        const preview = await evaluate(`window.graphicsBenchmark.run({
            full: true, width: 480, height: 360, samples: 32, depth: 5, seed: 20261112, budgetMs: 90_000,
        })`);
        assert.equal(preview.record.status, 'completed');
        await writeFile(join(output, 'valley-preview.png'), Buffer.from(preview.png.split(',')[1], 'base64'));
        await writeFile(join(output, 'preview.json'), JSON.stringify({ ...preview.record, cancellationVerified: true }, null, 2) + '\n');
        console.log(JSON.stringify({ ...preview.record, cancellationVerified: true }, null, 2));
        return;
    }
    const raster = await until(() => evaluate(`(() => {
        const alert = document.querySelector('[role=alert]');
        if (alert) throw new Error(alert.textContent);
        const canvas = document.querySelector('canvas');
        if (!canvas) return;
        const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
        const colors = new Set();
        for (let i = 0; i < data.length; i += 84) colors.add(data.slice(i, i + 3).join(','));
        if (colors.size > 100) return { colors: colors.size, image: canvas.toDataURL() };
    })()`), 60_000);
    const environment = await evaluate('({ isolated: crossOriginIsolated, sharedArrayBuffer: typeof SharedArrayBuffer, hardwareConcurrency: navigator.hardwareConcurrency })');
    const save = (name, image) => writeFile(join(output, name), Buffer.from(image.split(',')[1], 'base64'));
    await save('raster.png', raster.image);
    await evaluate(`(() => {
        document.querySelector('[role=dialog] [data-slot=dialog-close]')?.click();
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    })()`);
    await until(() => evaluate("!document.querySelector('[role=dialog][data-state=open]')"));

    if (mode === '--walkthrough') {
        const button = text => `[...document.querySelectorAll('button')].find(button => button.textContent.trim() === ${JSON.stringify(text)})`;
        const click = expression => evaluate(`(() => {
            const control = ${expression};
            if (!control || control.disabled) throw new Error('Control unavailable: ' + ${JSON.stringify(expression)});
            control.click();
        })()`);
        const clickText = text => click(button(text));
        const select = selector => `document.querySelector(${JSON.stringify(selector)})`;
        const set = (selector, value) => evaluate(`(() => {
            const input = ${select(selector)};
            if (!input || input.disabled) throw new Error('Input unavailable: ' + ${JSON.stringify(selector)});
            Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, ${JSON.stringify(String(value))});
            input.dispatchEvent(new Event('input', { bubbles: true }));
            input.dispatchEvent(new Event('change', { bubbles: true }));
        })()`);
        const press = async key => {
            await page('Input.dispatchKeyEvent', { type: 'keyDown', key, code: key });
            await page('Input.dispatchKeyEvent', { type: 'keyUp', key, code: key });
        };
        const choose = async (trigger, label) => {
            await evaluate(`${select(trigger)}.focus()`);
            await press('ArrowDown');
            const option = `[...document.querySelectorAll('[role=option]')].find(option => option.textContent.trim() === ${JSON.stringify(label)})`;
            await until(() => evaluate(`Boolean(${option})`));
            await click(option);
        };
        const status = phase => evaluate(`document.querySelector('[data-render-phase="${phase}"]')?.textContent`);
        const image = 'document.querySelector("canvas").toDataURL()';
        const getImage = () => evaluate(image);
        const settleFrame = () => evaluate('new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())))');
        let lastRaster = raster.image;
        let lastTrace;
        const steps = [];

        async function capture(name, expectChange = true) {
            await settleFrame();
            const currentRaster = await until(async () => {
                const current = await getImage();
                return (!expectChange || current !== lastRaster) && current;
            });
            await set('#sample-target', 2);
            await clickText('Start render');
            assert.match(await until(() => status(2), 90_000), /Complete.*2\/2 spp/);
            const trace = await getImage();
            if (lastTrace) {
                if (expectChange) assert.notEqual(trace, lastTrace, `Path trace should change after ${name}`);
                else assert.equal(trace, lastTrace, 'Restart should reproduce the previous completed render');
            }
            await save(`${name}-raster.png`, currentRaster);
            await save(`${name}-trace.png`, trace);
            await clickText('Back to editor');
            lastRaster = await until(async () => {
                const current = await getImage();
                return current !== trace && current;
            });
            lastTrace = trace;
            steps.push(name);
            console.log(`Editor walkthrough: ${name}`);
        }
        const open = async label => {
            if (await evaluate(`(${button(label)})?.getAttribute('aria-expanded') !== 'true'`)) await clickText(label);
        };
        const transform = async () => {
            await open('Edit Selected Object');
            await open('Transform');
        };

        await capture('initial', false);
        await open('Add New Object');
        await choose('#object-type-select', 'Box');
        await set('#box-length', 2.5);
        await set('#box-height', 1.6);
        await clickText('Add Object to Scene');
        await capture('add-box');
        await transform();
        await set('#pos-increment', 0.7);
        await click(select('[aria-label="Increase position Z"]'));
        await capture('translate');
        await transform();
        await set('#rot-increment', 30);
        await click(select('[aria-label="Increase rotation X"]'));
        await capture('rotate-x');
        await transform();
        await set('#scale-increment', 0.25);
        await click(select('#transform-scale-increment'));
        await capture('scale');
        await open('Edit Selected Object');
        await open('Material Editor');
        await choose('#material-type', 'Metal');
        await set('#material-color', '#da3f2d');
        await capture('material');
        await evaluate(`${select('#fov-slider [role=slider]')}.focus()`);
        for (let i = 0; i < 6; i++) await press('ArrowRight');
        await capture('camera-fov');
        await evaluate('document.activeElement.blur()');
        await page('Input.dispatchKeyEvent', { type: 'keyDown', key: 'w', code: 'KeyW', windowsVirtualKeyCode: 87 });
        try {
            await until(async () => await getImage() !== lastRaster);
        } finally {
            await page('Input.dispatchKeyEvent', { type: 'keyUp', key: 'w', code: 'KeyW', windowsVirtualKeyCode: 87 });
        }
        await capture('camera-move');
        await open('Add New Object');
        await choose('#object-type-select', 'Custom (.glb) - experimental!');
        await page('DOM.enable');
        const { root } = await page('DOM.getDocument');
        const { nodeId } = await page('DOM.querySelector', { nodeId: root.nodeId, selector: '#custom-file-upload' });
        await page('DOM.setFileInputFiles', { nodeId, files: [resolve('public/static/roza_bust.glb')] });
        await clickText('Add Object to Scene');
        await capture('import-glb');
        await set('#sample-target', 4096);
        await clickText('Start render');
        await until(() => evaluate("document.querySelector('progress')?.value > 0"));
        assert.equal(await evaluate(`${select('#fov-slider [role=slider]')}.getAttribute('data-disabled') !== null`), true);
        await clickText('Cancel render');
        assert.match(await until(() => status(3)), /Cancelled.*\/4096 spp/);
        await until(async () => await getImage() === lastRaster);
        await capture('restart-after-cancel', false);
        await set('#sample-target', 8);
        await clickText('Start render');
        assert.match(await until(() => status(2), 90_000), /Complete.*8\/8 spp/);
        await save('path-traced.png', await getImage());
        const { data } = await page('Page.captureScreenshot', { format: 'png' });
        await writeFile(join(output, 'editor.png'), Buffer.from(data, 'base64'));
        const workers = await evaluate("[...document.querySelectorAll('p')].find(p => p.textContent.startsWith('Rayon workers')).textContent");
        assert.match(workers, /1\/1/);
        const report = { ...environment, browser: version.product, workers, steps, cancelled: true, repeatImageExact: true };
        await writeFile(join(output, 'walkthrough.json'), JSON.stringify(report, null, 2) + '\n');
        console.log(JSON.stringify(report, null, 2));
        return;
    }
    await evaluate(`(() => {
        const start = [...document.querySelectorAll('button')].find(button => ['Ray Trace', 'Start render'].includes(button.textContent.trim()));
        if (!start) throw new Error('Start render button unavailable');
        start.click();
    })()`);
    const traced = await until(() => evaluate(`(() => {
        const image = document.querySelector('canvas').toDataURL();
        if (image !== ${JSON.stringify(raster.image)}) return image;
    })()`), 90_000);
    await save('path-traced.png', traced);
    console.log(JSON.stringify({ ...environment, browser: version.product, rasterColors: raster.colors }, null, 2));
});

if (/^https?:/.test(url)) await smoke(url);
else await withPreview(url, smoke);
