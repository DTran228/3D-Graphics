export async function getFileBytes(url: string): Promise<Uint8Array | undefined> {
    try {
        const response = await fetch(url);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        return new Uint8Array(await response.arrayBuffer());
    } catch (error) {
        console.error('Error fetching model:', url, error);
        return undefined;
    }
}
