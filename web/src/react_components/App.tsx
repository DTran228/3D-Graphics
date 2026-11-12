import React from 'react';
import MainCanvas from './MainCanvas';
import SceneControlPanel from './SceneControlPanel/SceneControlPanel';
import { GameProvider } from '@/gameContext';
import { ThemeProvider } from "@/components/theme-provider"
import type { Runtime } from '@/benchmark';

const App = ({ runtime }: { runtime: Runtime }) => {
    return (
        <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
            <div className="flex flex-row h-screen w-screen overflow-hidden bg-background text-foreground">
                {/* Sidebar */}
                <div className="w-[250px] flex-shrink-0 border-r border-border bg-card h-full">
                    <GameProvider>
                        <SceneControlPanel runtime={runtime} />
                    </GameProvider>
                </div>

                {/* Main Canvas Area */}
                <div className="flex-grow flex justify-center h-full p-4 bg-muted/20"> {/* Added padding and a slightly different bg for canvas area */}
                    <MainCanvas />
                </div>
            </div>
        </ThemeProvider>
    );
};

export default App;
