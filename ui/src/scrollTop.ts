/** Scroll the dashboard's content area back to the top, e.g. on a new view. */
export function scrollMainToTop(): void {
    document.querySelector(".app-main-content")?.scrollTo({ top: 0 });
}
