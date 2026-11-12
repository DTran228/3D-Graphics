import { useEffect, useRef, useState } from 'react';
import { BenchmarkRender } from '@wasm/wasm_graphics';

export const configurations = {
    A: { algorithm: 'brute-force', threads: 1, label: 'A · Brute-force, 1 worker' },
    B: { algorithm: 'bvh', threads: 1, label: 'B · BVH, 1 worker' },
    C: { algorithm: 'bvh', threads: 8, label: 'C · BVH, 8 workers' },
} as const;

export type Configuration = keyof typeof configurations;
export type Runtime = {
    requestedThreads: number;
    actualThreads: number;
    hardwareConcurrency: number;
    wasmLoadMs: number;
    poolInitMs: number;
};

type Settings = {
    full: boolean;
    width: number;
    height: number;
    samples: number;
    depth: number;
    seed: number;
    budgetMs: number;
};

const defaults: Settings = {
    full: false, width: 96, height: 72, samples: 2, depth: 5, seed: 20261112, budgetMs: 90_000,
};

function yieldToBrowser() {
    // A timer gives input and React updates a chance between render batches.
    return new Promise<void>(resolve => setTimeout(resolve, 0));
}

async function runBenchmark(
    configuration: Configuration, runtime: Runtime, settings: Settings,
    signal: AbortSignal, onProgress: (samples: number) => void,
) {
    if (!Number.isInteger(settings.seed) || settings.seed < 0 || settings.seed > 0xffffffff ||
        ![settings.width, settings.height, settings.samples, settings.depth].every(Number.isInteger) ||
        !Number.isFinite(settings.budgetMs) || settings.budgetMs <= 0) {
        throw new Error('Enter whole numbers for resolution, samples, depth and the 32-bit seed.');
    }
    const { full, width, height, samples, depth, seed, budgetMs } = settings;
    const start = performance.now();
    const renderer = new BenchmarkRender(full, width, height, samples, depth, seed);
    try {
        const sceneHash = renderer.scene_hash();
        const triangles = renderer.triangle_count();
        const fixtureMs = performance.now() - start;
        const prepareStart = performance.now();
        renderer.prepare(configuration !== 'A');
        const renderStart = performance.now();
        const accelBuildMs = renderStart - prepareStart;
        let progress = 0;
        let renderMs = 0;
        let status: 'completed' | 'cancelled' | 'timeout' = 'completed';
        while (progress < samples) {
            if (signal.aborted || performance.now() - start >= budgetMs) {
                status = signal.aborted ? 'cancelled' : 'timeout';
                break;
            }
            const stepStart = performance.now();
            progress = renderer.step();
            renderMs += performance.now() - stepStart;
            onProgress(progress);
            if (progress < samples) await yieldToBrowser();
        }
        const end = performance.now();
        if (status === 'completed' && end - start >= budgetMs) status = 'timeout';
        const renderWallMs = end - renderStart;
        const runWallMs = end - start;
        return {
            record: {
                ...runtime, configuration, ...settings, scene: full ? 'valley-100k-v1' : 'valley-640-v1',
                algorithm: configurations[configuration].algorithm, sceneHash, triangles, status,
                completedSamples: Math.floor(progress), progress, fixtureMs, accelBuildMs, renderMs,
                renderWallMs, runWallMs, totalMs: runtime.wasmLoadMs + runtime.poolInitMs + runWallMs,
                visible: document.visibilityState === 'visible', browser: navigator.userAgent,
            },
            rgba: renderer.rgba(),
            linear: status === 'completed' ? renderer.linear_rgb() : null,
        };
    } finally {
        renderer.free();
    }
}

type Result = Awaited<ReturnType<typeof runBenchmark>>['record'];

function draw(canvas: HTMLCanvasElement, width: number, height: number, rgba: Uint8Array) {
    canvas.width = width;
    canvas.height = height;
    canvas.getContext('2d')!.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
}

export function BenchmarkPage({ configuration, runtime }: { configuration: Configuration; runtime: Runtime }) {
    const [settings, setSettings] = useState(defaults);
    const [progress, setProgress] = useState(0);
    const [target, setTarget] = useState(defaults.samples);
    const [running, setRunning] = useState(false);
    const [result, setResult] = useState<Result | null>(null);
    const [error, setError] = useState('');
    const canvas = useRef<HTMLCanvasElement>(null);
    const cancellation = useRef<AbortController | null>(null);

    async function start(options: Settings, exportPixels = false) {
        if (cancellation.current) throw new Error('A render is already running.');
        const controller = new AbortController();
        cancellation.current = controller;
        setError('');
        setResult(null);
        setProgress(0);
        setTarget(options.samples);
        setRunning(true);
        try {
            // Give React a chance to show the running state before generating geometry.
            await yieldToBrowser();
            const { record, rgba, linear } = await runBenchmark(configuration, runtime, options, controller.signal, setProgress);
            draw(canvas.current!, options.width, options.height, rgba);
            setResult(record);
            return {
                record,
                png: exportPixels && record.status === 'completed' ? canvas.current!.toDataURL() : null,
                linear: exportPixels && linear ? Array.from(linear) : null,
            };
        } catch (cause) {
            setError(String(cause));
            throw cause;
        } finally {
            cancellation.current = null;
            setRunning(false);
        }
    }

    // Browser runner uses this same entry point as the visible controls.
    useEffect(() => {
        Object.assign(window, { graphicsBenchmark: { run: (options: Settings) => start(options, true), runtime } });
        return () => { Reflect.deleteProperty(window, 'graphicsBenchmark'); };
    }, [configuration, runtime]);

    function downloadResult() {
        const url = URL.createObjectURL(new Blob([JSON.stringify(result, null, 2)], { type: 'application/json' }));
        const link = document.createElement('a');
        link.href = url;
        link.download = `graphics-${configuration}.json`;
        link.click();
        setTimeout(() => URL.revokeObjectURL(url), 0);
    }

    return (
        <main className="mx-auto max-w-4xl space-y-4 p-6">
            <a href="./" className="underline">Scene editor</a>
            <h1 className="text-2xl font-semibold">Path tracing benchmark</h1>
            <p>Rayon workers {runtime.actualThreads} / {runtime.requestedThreads} requested · Browser reports {runtime.hardwareConcurrency} logical processors</p>
            <div className="flex flex-wrap gap-4">
                <label className="grid gap-1">Configuration
                    <select className="rounded border p-1" value={configuration} disabled={running}
                        onChange={event => { location.search = `?benchmark=${event.target.value}`; }}>
                        {Object.entries(configurations).map(([id, config]) => <option key={id} value={id}>{config.label}</option>)}
                    </select>
                </label>
                <label className="grid gap-1">Scene
                    <select className="rounded border p-1" value={settings.full ? 'full' : 'small'} disabled={running}
                        onChange={event => setSettings({ ...settings, full: event.target.value === 'full' })}>
                        <option value="small">Valley · 640 triangles</option>
                        <option value="full">Valley · 100,000 triangles</option>
                    </select>
                </label>
            </div>
            <div className="flex flex-wrap gap-4">
                {([
                    ['width', 'Width', 512], ['height', 'Height', 512], ['samples', 'Samples per pixel', 4096],
                    ['depth', 'Max path depth', 64], ['seed', 'Seed', 0xffffffff],
                ] as const).map(([key, label, max]) => (
                    <label key={key} className="grid gap-1">{label}
                        <input className="w-28 rounded border p-1" type="number" min={key === 'seed' ? 0 : 1}
                            max={max} value={settings[key]} disabled={running}
                            onChange={event => setSettings({ ...settings, [key]: Number(event.target.value) })} />
                    </label>
                ))}
            </div>
            <div className="flex gap-3">
                {running
                    ? <button className="rounded border px-3 py-1" onClick={() => cancellation.current?.abort()}>Cancel benchmark</button>
                    : <button className="rounded border px-3 py-1" onClick={() => void start(settings).catch(() => {})}>Start benchmark</button>}
                {result && <button className="rounded border px-3 py-1" onClick={downloadResult}>Download result</button>}
                {result?.status === 'completed' && (
                    <a className="rounded border px-3 py-1" href={canvas.current?.toDataURL()} download="valley.png">Download image</a>
                )}
            </div>
            {running && <progress className="w-full" aria-label="Benchmark progress" max={target} value={progress} />}
            {error && <p role="alert">{error}</p>}
            {result && <p role="status">
                {result.status} · {result.completedSamples}/{result.samples} spp · {(result.renderWallMs / 1000).toFixed(2)} s render wall time
            </p>}
            <canvas ref={canvas} width={96} height={72} aria-label="Benchmark image"
                className="w-[480px] max-w-full bg-neutral-900 [image-rendering:pixelated]" />
        </main>
    );
}
