import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface WindowChrome {
    isMaximized: boolean;
}

export function useWindowChrome(): WindowChrome {
    const [isMaximized, setIsMaximized] = useState(false);

    useEffect(() => {
        const win = getCurrentWindow();
        let unlisten: (() => void) | undefined;
        let cancelled = false;

        void win.isMaximized().then((max) => {
            if (!cancelled) setIsMaximized(max);
        });

        void win.onResized(() => {
            void win.isMaximized().then((max) => {
                if (!cancelled) setIsMaximized(max);
            });
        }).then((fn) => {
            unlisten = fn;
        });

        return () => {
            cancelled = true;
            unlisten?.();
        };
    }, []);

    return { isMaximized };
}
