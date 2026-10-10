import { durationParts } from "../format";

/** "1h 5m" as a big number with small units. */
export function Amount({ seconds }: { seconds: number }) {
    return (
        <>
            {durationParts(seconds).map(([value, unit], i) => (
                <span key={i} className="tt-amount-part">
                    {value}
                    <small>{unit}</small>
                </span>
            ))}
        </>
    );
}
