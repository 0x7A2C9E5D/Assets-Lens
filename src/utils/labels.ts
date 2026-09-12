/**
 * Display label for a visual asset: `name(guid)`.
 *
 * A visual's identity is its GUID: `VisualBank` maps name -> id in a HashMap, so several resources
 * can share a single name and the list is deliberately not deduplicated by name. Carrying the GUID
 * next to the name is what makes those rows tellable apart.
 */
export function visualLabel(name: string, id: string): string {
    return `${name}(${id})`
}
