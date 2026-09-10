// The icon set: our own drawings on a 24-unit grid, stroked, one component
// each (architecture v2.1 §6.1: no third-party icon set). Keep them simple
// enough to read at sixteen pixels.

import type { SVGProps } from "react";

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, "children"> {
  size?: number;
}

function icon(name: string, paths: readonly string[], extra?: readonly [string, number, number, number][]) {
  const Icon = ({ size = 16, ...rest }: IconProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      data-icon={name}
      {...rest}
    >
      {paths.map((d, i) => (
        <path key={i} d={d} />
      ))}
      {extra?.map(([kind, a, b, c], i) =>
        kind === "circle" ? <circle key={`c${i}`} cx={a} cy={b} r={c} /> : null,
      )}
    </svg>
  );
  Icon.displayName = name;
  return Icon;
}

export const ChatIcon = icon("chat", ["M4 5h16v11H9l-5 4V5z"]);
export const AgentIcon = icon("agent", ["M8 3h8l1 4H7l1-4z", "M5 7h14v10H5z", "M9 21h6", "M12 17v4"], [["circle", 9.5, 12, 1], ["circle", 14.5, 12, 1]]);
export const ToolsIcon = icon("tools", ["M14 4l6 6-3 3-6-6 3-3z", "M11 7l-7 7v4h4l7-7", "M5 21l-2-2"]);
export const ModelsIcon = icon("models", ["M4 7l8-4 8 4-8 4-8-4z", "M4 7v10l8 4 8-4V7", "M12 11v10"]);
export const DataIcon = icon("data", ["M12 3c4.4 0 8 1.3 8 3s-3.6 3-8 3-8-1.3-8-3 3.6-3 8-3z", "M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6", "M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"]);
export const HistoryIcon = icon("history", ["M3 12a9 9 0 1 0 3-6.7", "M3 4v5h5", "M12 7v5l3 2"]);
export const LibraryIcon = icon("library", ["M4 4h4v16H4z", "M10 4h4v16h-4z", "M16 5l4 1-3 14-4-1 3-14z"]);
export const SettingsIcon = icon("settings", ["M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z", "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"]);
export const HelpIcon = icon("help", ["M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .9-1 1.7", "M12 17h.01"], [["circle", 12, 12, 9]]);
export const ChevronDownIcon = icon("chevron-down", ["M6 9l6 6 6-6"]);
export const ChevronRightIcon = icon("chevron-right", ["M9 6l6 6-6 6"]);
export const MinusIcon = icon("minus", ["M5 12h14"]);
export const PlusIcon = icon("plus", ["M12 5v14", "M5 12h14"]);
export const SparkIcon = icon("spark", ["M12 3l2 5.5L19.5 10 14 12l-2 5.5L10 12 4.5 10 10 8.5 12 3z", "M19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8L19 16z"]);
export const PinIcon = icon("pin", ["M9 3h6l-1 6 3 3v1H7v-1l3-3-1-6z", "M12 13v8"]);
export const PlayIcon = icon("play", ["M7 4l13 8-13 8V4z"]);
export const SearchIcon = icon("search", ["M20 20l-4.3-4.3"], [["circle", 10.5, 10.5, 6.5]]);
export const PanelsIcon = icon("panels", ["M3 4h18v16H3z", "M3 10h18", "M12 10v10"]);
export const SidebarIcon = icon("sidebar", ["M3 4h18v16H3z", "M15 4v16"]);
export const CloseIcon = icon("close", ["M6 6l12 12", "M18 6L6 18"]);
export const DownloadIcon = icon("download", ["M12 4v11", "M7 10l5 5 5-5", "M4 19h16"]);
export const TrashIcon = icon("trash", ["M4 7h16", "M9 7V4h6v3", "M6 7l1 13h10l1-13", "M10 11v6", "M14 11v6"]);
export const SpinnerIcon = icon("spinner", ["M12 3a9 9 0 0 1 9 9"]);
export const ClipIcon = icon("clip", ["M17 7l-8.5 8.5a2.1 2.1 0 0 0 3 3L20 10a4.2 4.2 0 0 0-6-6l-9.5 9.5a6.4 6.4 0 0 0 9 9L20 16"]);
export const SendIcon = icon("send", ["M4 12h13", "M13 6l6 6-6 6", "M4 6v12"]);
export const SlidersIcon = icon("sliders", ["M4 7h10", "M18 7h2", "M4 17h4", "M12 17h8"], [["circle", 16, 7, 2], ["circle", 10, 17, 2]]);
export const StopIcon = icon("stop", ["M6 6h12v12H6z"]);
export const CheckIcon = icon("check", ["M5 12l5 5L20 7"]);
export const FileIcon = icon("file", ["M6 3h8l5 5v13H6z", "M14 3v5h5", "M9 13h6", "M9 17h6"]);
export const GlobeIcon = icon("globe", ["M3 12h18", "M12 3a14 14 0 0 1 0 18", "M12 3a14 14 0 0 0 0 18"], [["circle", 12, 12, 9]]);
export const NetworkIcon = icon("network", ["M12 5v4", "M6 15V9h12v6", "M6 19v-4", "M18 19v-4"], [["circle", 12, 4, 1.5], ["circle", 6, 20, 1.5], ["circle", 18, 20, 1.5]]);
export const BoxesIcon = icon("boxes", ["M3 8l4.5-3 4.5 3-4.5 3L3 8z", "M12 8l4.5-3 4.5 3-4.5 3L12 8z", "M7.5 16l4.5-3 4.5 3-4.5 3-4.5-3z", "M3 8v8l4.5 3", "M21 8v8l-4.5 3", "M7.5 11v5", "M16.5 11v5"]);
export const UndoIcon = icon("undo", ["M9 14L4 9l5-5", "M4 9h10a6 6 0 0 1 0 12h-3"]);
export const RedoIcon = icon("redo", ["M15 14l5-5-5-5", "M20 9H10a6 6 0 0 0 0 12h3"]);
export const HandIcon = icon("hand", ["M8 13V6a1.5 1.5 0 0 1 3 0v5", "M11 11V4a1.5 1.5 0 0 1 3 0v7", "M14 11V5.5a1.5 1.5 0 0 1 3 0V13", "M17 13V9.5a1.5 1.5 0 0 1 3 0V15a6 6 0 0 1-6 6h-2.5a5 5 0 0 1-4-2L4.5 14a1.5 1.5 0 0 1 2.4-1.8L8 13"]);
export const CursorIcon = icon("cursor", ["M5 3l14 8-6 2-3 6L5 3z"]);
export const StickyIcon = icon("sticky", ["M4 4h16v10l-6 6H4z", "M14 20v-6h6"]);
export const RectIcon = icon("rect", ["M4 6h16v12H4z"]);
export const EllipseIcon = icon("ellipse", [], [["circle", 12, 12, 8]]);
export const TextIcon = icon("text", ["M5 6h14", "M12 6v14", "M9 20h6"]);
export const ArrowIcon = icon("arrow", ["M4 20L20 4", "M11 4h9v9"]);
export const PenIcon = icon("pen", ["M4 20c4-1 6-3 8-7s4-6 8-7", "M14 6l4 4"]);
export const FrameIcon = icon("frame", ["M6 3v18", "M18 3v18", "M3 6h18", "M3 18h18"]);
export const FitIcon = icon("fit", ["M4 9V4h5", "M20 9V4h-5", "M4 15v5h5", "M20 15v5h-5"]);
export const LockIcon = icon("lock", ["M6 11h12v10H6z", "M8 11V7a4 4 0 0 1 8 0v4"]);
export const UnlockIcon = icon("unlock", ["M6 11h12v10H6z", "M8 11V7a4 4 0 0 1 7.5-2"]);
export const CopyIcon = icon("copy", ["M9 9h11v11H9z", "M5 15V4h11"]);
export const LayersIcon = icon("layers", ["M12 3l9 5-9 5-9-5 9-5z", "M3 13l9 5 9-5", "M3 17l9 5 9-5"]);
export const MoreIcon = icon("more", [], [["circle", 6, 12, 1.5], ["circle", 12, 12, 1.5], ["circle", 18, 12, 1.5]]);
export const MarkIcon = icon("mark", ["M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16z", "M12 8v8", "M8 12h8"]);
export const EditIcon = icon("edit", ["M4 20h4l11-11-4-4L4 16v4z", "M13 7l4 4"]);
export const ChevronLeftIcon = icon("chevron-left", ["M15 6l-6 6 6 6"]);
export const CircleIcon = icon("circle", [], [["circle", 12, 12, 7]]);
