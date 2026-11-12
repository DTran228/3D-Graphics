import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

function stats(values) {
    const ordered = [...values].sort((a, b) => a - b);
    const middle = Math.floor(ordered.length / 2);
    return {
        median: ordered.length % 2 ? ordered[middle] : (ordered[middle - 1] + ordered[middle]) / 2,
        min: ordered[0],
        max: ordered.at(-1),
    };
}

export function summarize(records) {
    const measurements = records.filter(run => run.phase === 'measurement');
    const configurations = Object.fromEntries(['A', 'B', 'C'].map(configuration => {
        const matching = measurements.filter(run => run.configuration === configuration && run.status === 'completed');
        return [configuration, {
            count: matching.length,
            ...(matching.length && Object.fromEntries([
                'wasmLoadMs', 'poolInitMs', 'fixtureMs', 'accelBuildMs', 'renderMs', 'renderWallMs', 'totalMs',
            ].map(field => [field, stats(matching.map(run => run[field]))]))),
        }];
    }));
    const failures = records.filter(run => run.status !== 'completed' || !run.imageComparison?.passes)
        .map(({ runId, status, error, imageComparison }) => ({ runId, status, error, imageComparison }));
    const sameSettings = ['sceneHash', 'triangles', 'seed', 'width', 'height', 'samples', 'depth', 'wasmSha256']
        .every(field => new Set(records.map(run => run[field])).size === 1);
    const valid = sameSettings && failures.length === 0 && Object.values(configurations).every(group => group.count >= 5);
    const speedups = valid ? Object.fromEntries(['renderMs', 'renderWallMs', 'totalMs'].map(metric => {
        const { A, B, C } = configurations;
        return [metric, {
            bruteForceToBvh: A[metric].median / B[metric].median,
            oneToEightWorkers: B[metric].median / C[metric].median,
            bruteForceToEightWorkers: A[metric].median / C[metric].median,
        }];
    })) : null;
    return { configurations, sameSettings, failures, speedups };
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
    const records = JSON.parse(await readFile(process.argv[2], 'utf8'));
    console.log(JSON.stringify(summarize(records), null, 2));
}
