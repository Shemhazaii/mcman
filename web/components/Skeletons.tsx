export function SkBar({ width, height = 11 }: { width: string; height?: number }) {
  return <span className="sk" style={{ width, height: `${height}px` }} />;
}

export function ModSkeletonRows({ count = 8 }: { count?: number }) {
  return (
    <>
      {Array.from({ length: count }).map((_, index) => (
        <tr key={index}>
          <td style={{ padding: "8px 5px" }}><SkBar width="14px" height={14} /></td>
          <td style={{ padding: "8px 5px" }}><SkBar width="18px" height={18} /></td>
          <td style={{ padding: "8px 5px" }}><SkBar width="46px" height={46} /></td>
          <td>
            <div className="skrow">
              <SkBar width="65%" height={11} />
              <SkBar width="38%" height={9} />
            </div>
          </td>
          <td style={{ padding: "8px" }}><SkBar width="72px" height={13} /></td>
          <td style={{ padding: "8px" }}><SkBar width="56px" height={11} /></td>
          <td className="dim"><SkBar width="52px" height={11} /></td>
          <td className="dim"><SkBar width="64px" height={11} /></td>
          <td />
        </tr>
      ))}
    </>
  );
}

export function HitSkeleton({ count = 5 }: { count?: number }) {
  return (
    <>
      {Array.from({ length: count }).map((_, index) => (
        <div className="hit" key={index}>
          <div style={{ flex: 1 }}>
            <div className="skrow">
              <SkBar width="42%" height={12} />
              <SkBar width="72%" height={10} />
              <SkBar width="30%" height={9} />
            </div>
          </div>
          <SkBar width="78px" height={24} />
        </div>
      ))}
    </>
  );
}
