/**
 * Adult domains are listed but not shown until the PIN is entered, so a
 * child browsing the block list doesn't get a directory of sites. Shared by
 * the full Websites view and the Limits summary card.
 */
export function isHiddenDomain(domain: string): boolean {
    return /(porn|xvideo|xnxx|xhamster|chaturbate|stripchat|onlyfans|redtube|tubegalore|eporner|spankbang|rule34|xxx)/i.test(domain);
}
