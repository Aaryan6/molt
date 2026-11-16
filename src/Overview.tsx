import type { DiskSpace } from "./api";
import { formatBytes, GROUPS, isIdle, splitBytes, totalSize, type Item } from "./model";

type Props = {
  items: Item[];
  disk: DiskSpace | null;
  drive: string;
  onSelectIdle: () => void;
};

export default function Overview({ items, disk, drive, onSelectIdle }: Props) {
  return (
    <section className="overview">
      <DiskCard items={items} disk={disk} drive={drive} />
      <ReclaimCard items={items} />
      <IdleCard items={items} onSelectIdle={onSelectIdle} />
    </section>
  );
}

function BigNumber({ bytes }: { bytes: number }) {
  const [value, unit] = splitBytes(bytes);
  return (
    <div className="big num">
      {value}
      <small>{unit}</small>
    </div>
  );
}

function DiskCard({ items, disk, drive }: { items: Item[]; disk: DiskSpace | null; drive: string }) {
  const dev = totalSize(items);
  const used = disk ? disk.total - disk.free : 0;
  const other = Math.max(0, used - dev);
  return (
    <div className="card">
      <div className="card-h">
        <span>Disk {drive}</span>
        <span className="tag">{disk ? formatBytes(disk.total) : ""}</span>
      </div>
      <div className="disk-body">
        <div className="ring">
          <Donut
            total={disk?.total ?? 1}
            segments={
              disk
                ? [
                    { value: other, color: "var(--seg-other)" },
                    { value: dev, color: "var(--accent)" },
                  ]
                : []
            }
          />
          <div className="center">
            <b className="num">{disk ? formatBytes(disk.free) : "—"}</b>
            <span>free</span>
          </div>
        </div>
        <ul className="legend-v">
          <li>
            <i className="dot" style={{ background: "var(--accent)" }} />
            Dev folders<b className="num">{formatBytes(dev)}</b>
          </li>
          <li>
            <i className="dot" style={{ background: "var(--seg-other)" }} />
            Everything else<b className="num">{disk ? formatBytes(other) : "—"}</b>
          </li>
          <li>
            <i className="dot" style={{ background: "var(--card-2)" }} />
            Free<b className="num">{disk ? formatBytes(disk.free) : "—"}</b>
          </li>
        </ul>
      </div>
    </div>
  );
}

function Donut({ segments, total, size = 112, stroke = 12 }: {
  segments: { value: number; color: string }[];
  total: number;
  size?: number;
  stroke?: number;
}) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  let offset = 0;
  return (
    <svg width={size} height={size} style={{ transform: "rotate(-90deg)" }}>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--card-2)" strokeWidth={stroke} />
      {segments.map((s, i) => {
        const len = (s.value / total) * c;
        const el = (
          <circle
            key={i}
            cx={size / 2}
            cy={size / 2}
            r={r}
            fill="none"
            stroke={s.color}
            strokeWidth={stroke}
            style={{ strokeDasharray: `${Math.max(len - 1.5, 1.5)} ${c}`, strokeDashoffset: -offset }}
          />
        );
        offset += len;
        return el;
      })}
    </svg>
  );
}

function ReclaimCard({ items }: { items: Item[] }) {
  const projects = new Set(items.map((i) => i.path.slice(0, i.path.length - i.folder.length))).size;
  const byGroup = GROUPS.map((g) => ({ g, size: totalSize(items.filter((i) => i.group === g)) })).filter(
    (x) => x.size > 0,
  );
  return (
    <div className="card">
      <div className="card-h">
        <span>Reclaimable</span>
        <span className="tag">
          {items.length} folders · {projects} projects
        </span>
      </div>
      <BigNumber bytes={totalSize(items)} />
      <div className="segbar">
        {byGroup.map(({ g, size }) => (
          <i key={g.id} style={{ flexGrow: size, background: g.color }} title={`${g.label} ${formatBytes(size)}`} />
        ))}
      </div>
      <div className="legend-g">
        {byGroup.map(({ g, size }) => (
          <div key={g.id}>
            <i className="dot" style={{ background: g.color }} />
            {g.label}
            <b className="num">{formatBytes(size)}</b>
          </div>
        ))}
      </div>
    </div>
  );
}

const BUCKETS: { label: string; idle: boolean; test: (d: number) => boolean }[] = [
  { label: "< 1 wk", idle: false, test: (d) => d < 7 },
  { label: "< 1 mo", idle: false, test: (d) => d >= 7 && d < 30 },
  { label: "1–6 mo", idle: true, test: (d) => d >= 30 && d < 180 },
  { label: "6 mo +", idle: true, test: (d) => d >= 180 },
];

function IdleCard({ items, onSelectIdle }: { items: Item[]; onSelectIdle: () => void }) {
  const idle = items.filter(isIdle);
  const buckets = BUCKETS.map((b) => ({
    ...b,
    size: totalSize(items.filter((i) => i.age !== null && b.test(i.age))),
  }));
  const max = Math.max(1, ...buckets.map((b) => b.size));
  return (
    <div className="card">
      <div className="card-h">
        <span>Untouched projects</span>
        <button className="link" onClick={onSelectIdle} disabled={idle.length === 0}>
          Select
        </button>
      </div>
      <BigNumber bytes={totalSize(idle)} />
      <div className="sub">idle for 30+ days</div>
      <div className="hist">
        {buckets.map((b) => (
          <div className="col" key={b.label} title={`${b.label}: ${formatBytes(b.size)}`}>
            <div className={b.idle ? "bar idle" : "bar"} style={{ height: Math.max(4, (b.size / max) * 48) }} />
            <span className="lbl">{b.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
