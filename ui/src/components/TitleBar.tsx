import { getCurrentWindow } from '@tauri-apps/api/window';
import React, { useState, useEffect } from 'react';
import './TitleBar.css';

export function TitleBar() {
    const appWindow = getCurrentWindow();
    const [isMaximized, setIsMaximized] = useState(false);

    useEffect(() => {
        let unlisten: () => void;
        appWindow.onResized(async () => {
            const max = await appWindow.isMaximized();
            setIsMaximized(max);
        }).then(u => unlisten = u);

        // Initial check
        appWindow.isMaximized().then(setIsMaximized);

        return () => {
            if (unlisten) unlisten();
        };
    }, []);

    return (
        <div data-tauri-drag-region className="titlebar">
            <div className="titlebar-left" data-tauri-drag-region>
                <div className="titlebar-icon">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <circle cx="12" cy="12" r="10" />
                        <path d="M12 6v6l4 2" />
                    </svg>
                </div>
                <span className="titlebar-title" data-tauri-drag-region>Screentime</span>
            </div>
            
            <div className="titlebar-controls">
                <button 
                    className="titlebar-btn" 
                    onClick={() => appWindow.minimize()}
                    title="Minimize"
                >
                    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M1 5H9" stroke="currentColor" strokeLinecap="round" strokeWidth="1.5" />
                    </svg>
                </button>
                <button 
                    className="titlebar-btn" 
                    onClick={() => appWindow.toggleMaximize()}
                    title={isMaximized ? "Restore Down" : "Maximize"}
                >
                    {isMaximized ? (
                        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <rect x="2.5" y="1.5" width="6" height="6" stroke="currentColor" strokeWidth="1.5" />
                            <path d="M1.5 8.5V2.5H7.5" stroke="currentColor" strokeWidth="1.5" />
                        </svg>
                    ) : (
                        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <rect x="1.5" y="1.5" width="7" height="7" stroke="currentColor" strokeWidth="1.5" />
                        </svg>
                    )}
                </button>
                <button 
                    className="titlebar-btn titlebar-btn-close" 
                    onClick={() => appWindow.hide()}
                    title="Close"
                >
                    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M1.5 1.5L8.5 8.5M8.5 1.5L1.5 8.5" stroke="currentColor" strokeLinecap="round" strokeWidth="1.5" />
                    </svg>
                </button>
            </div>
        </div>
    );
}
