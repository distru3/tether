import { useCallback, useEffect, useState } from "react";

import { listAccounts } from "../api";
import type { AccountDto } from "../types/generated/AccountDto";

/** How often the account list refreshes (an account appears on first sign-in). */
const ACCOUNTS_MS = 60_000;

/**
 * The Windows accounts Tether knows (docs/adr/0005-per-account-usage.md).
 * `ownAccountId` comes from Status; a change refetches.
 */
export function useAccounts(ownAccountId: number | null): { accounts: AccountDto[]; refresh: () => void } {
    const [accounts, setAccounts] = useState<AccountDto[]>([]);
    const refresh = useCallback(() => {
        listAccounts()
            .then((dto) => setAccounts(dto.accounts))
            .catch(() => {});
    }, []);
    useEffect(() => {
        refresh();
        const id = window.setInterval(refresh, ACCOUNTS_MS);
        return () => window.clearInterval(id);
    }, [refresh, ownAccountId]);
    return { accounts, refresh };
}
