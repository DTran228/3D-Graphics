import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { arch, cpus, platform, release } from 'node:os';
import { join } from 'node:path';
import { until, withBrowser, withPreview } from './browser.mjs';
import { summarize } from './benchmark-summary.mjs';

const [browser, outDir, fixture, output, samplesArgument = '2'] = process.argv.slice(2);
const samples = Number(samplesArgument);
if (!browser || !outDir || !['small', 'full'].includes(fixture) || !output ||
    !Number.isInteger(samples) || samples < 1 || samples > 4096) {
    throw new Error('Usage: node scripts/benchmark.mjs CHROME VITE_OUT_DIR small|full OUTPUT_DIRECTORY [SPP]');
}
await mkdir(output, { recursive: true });
const rawPath = join(output, 'runs.json');
await writeFile(rawPath, '[]\n', { flag: 'wx' });

const digest = data => createHash('sha256').update(data).digest('hex');
const git = args => execFileSync('git', args, { cwd: '..', encoding: 'utf8' }).trim();
const sourcePaths = git(['ls-files', '-c', '-o', '--exclude-standard', '--',
    'wasm-graphics/src', 'wasm-graphics/Cargo.toml', 'wasm-graphics/Cargo.lock', 'wasm-graphics/build.ps1',
    'web/src', 'web/scripts/benchmark.mjs', 'web/scripts/benchmark-summary.mjs',
    'web/scripts/browser.mjs', 'web/package-lock.json', 'web/vite.config.ts']).split(/\r?\n/);
const sourceHash = createHash('sha256');
for (const name of [...new Set(sourcePaths)].sort()) {
    sourceHash.update(name + '\0').update(await readFile(join('..', name))).update('\0');
}
const wasmAssets = (await readdir(join(outDir, 'assets'))).filter(name => name.endsWith('.wasm'));
if (wasmAssets.length !== 1) throw new Error('Build Vite into a clean output directory with freshly generated WASM.');
const metadata = {
    revision: git(['rev-parse', 'HEAD']),
    sourceSha256: sourceHash.digest('hex'),
    wasmSha256: digest(await readFile(join(outDir, 'assets', wasmAssets[0]))),
    cpu: cpus()[0]?.model ?? null,
    os: `${platform()} ${release()} ${arch()}`,
    buildProfile: 'release',
    optimization: 'Cargo release defaults, +atomics,+bulk-memory; no wasm-opt',
    browserMode: 'Chromium headless, single foreground test page',
};
const settings = { full: fixture === 'full', width: 96, height: 72, samples, depth: 5, seed: 20261112, budgetMs: 90_000 };
const warmup = { ...settings, full: false, width: 32, height: 24, samples: 1 };
const tolerance = 1e-6;
const records = [];
const savedImages = new Set();
let reference;

function compareImage(runId, linear) {
    if (!linear || linear.length !== settings.width * settings.height * 3) {
        return { passes: false, reason: 'Incorrect linear RGB length' };
    }
    let min = Infinity;
    let max = -Infinity;
    for (const value of linear) {
        if (!Number.isFinite(value)) return { passes: false, reason: 'Nonfinite linear RGB' };
        min = Math.min(min, value);
        max = Math.max(max, value);
    }
    if (max < 0.1 || max - min < 0.05) return { passes: false, reason: 'Image is black or nearly constant' };
    if (!reference) reference = { runId, linear };
    let maxAbsError = 0;
    let sumSquaredError = 0;
    for (let index = 0; index < linear.length; index++) {
        const error = Math.abs(linear[index] - reference.linear[index]);
        maxAbsError = Math.max(maxAbsError, error);
        sumSquaredError += error * error;
    }
    const bytes = Buffer.alloc(linear.length * 4);
    linear.forEach((value, index) => bytes.writeFloatLE(value, index * 4));
    return {
        referenceRunId: reference.runId, tolerance, passes: maxAbsError <= tolerance,
        maxAbsError, rmse: Math.sqrt(sumSquaredError / linear.length),
        linearRgbSha256: digest(bytes), minLinear: min, maxLinear: max,
    };
}

await withPreview(outDir, origin => withBrowser(browser, async ({ page, evaluate, version }) => {
    await page('Page.enable');
    await page('Page.bringToFront');
    const chrome = version.product;

    async function run(configuration, phase, round) {
        const runId = `${fixture}-${phase}-${round}-${configuration}`;
        const startedAt = new Date().toISOString();
        const requestedThreads = configuration === 'C' ? 8 : 1;
        const initial = { ...metadata, ...settings, runId, phase, round, configuration, startedAt, chrome, requestedThreads };
        let record = initial;
        try {
            const { errorText } = await page('Page.navigate', { url: `${origin}?benchmark=${configuration}` });
            if (errorText) throw new Error(errorText);
            const runtime = await until(() => evaluate(`(() => {
                const alert = document.querySelector('[role=alert]');
                if (alert) throw new Error(alert.textContent);
                return window.graphicsBenchmark?.runtime;
            })()`));
            record = { ...record, ...runtime };
            if (runtime.actualThreads !== requestedThreads) throw new Error('Unexpected actual Rayon pool size');
            if (!await evaluate("document.visibilityState === 'visible'")) throw new Error('Benchmark page is hidden');
            const warm = await evaluate(`window.graphicsBenchmark.run(${JSON.stringify(warmup)})`);
            if (warm.record.status !== 'completed') throw new Error(`Warm-up ${warm.record.status}`);
            record.warmupRenderMs = warm.record.renderMs;
            const measured = await evaluate(`window.graphicsBenchmark.run(${JSON.stringify(settings)})`);
            record = { ...record, ...measured.record };
            if (!record.visible) throw new Error('Benchmark page became hidden');
            if (record.status === 'completed') {
                record.imageComparison = compareImage(runId, measured.linear);
                const png = Buffer.from(measured.png.split(',')[1], 'base64');
                record.pngSha256 = digest(png);
                if (!savedImages.has(configuration)) {
                    await writeFile(join(output, `${configuration}.png`), png);
                    savedImages.add(configuration);
                }
            }
        } catch (error) {
            record = { ...record, status: 'error', error: String(error) };
        }
        records.push(record);
        await writeFile(rawPath, JSON.stringify(records, null, 2) + '\n');
        console.log(`${runId}: ${record.status} ${record.renderMs?.toFixed(2) ?? '—'} ms compute, ${record.totalMs?.toFixed(2) ?? '—'} ms total`, record.error ?? '');
        return record;
    }

    const pilots = [];
    for (const config of ['A', 'B', 'C']) pilots.push(await run(config, 'pilot', 0));
    if (pilots.every(run => run.status === 'completed' && run.imageComparison?.passes)) {
        const orders = ['ABC', 'BCA', 'CAB', 'ACB', 'CBA'];
        for (const [round, order] of orders.entries()) {
            for (const config of order) await run(config, 'measurement', round + 1);
        }
    } else {
        console.warn('Pilot failed. Check runs.json. For a timeout, rerun all configurations with the same lower SPP.');
    }
}));

const summary = summarize(records);
await writeFile(join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
if (!summary.speedups) process.exitCode = 1;
