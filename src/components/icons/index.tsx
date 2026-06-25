import { forwardRef } from 'react'
import type { LucideIcon, LucideProps } from 'lucide-react'
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Copy,
  Download,
  Eye,
  EyeOff,
  File,
  Folder,
  FolderOpen,
  GripVertical,
  Info,
  LoaderCircle,
  Lock,
  MoonStar,
  PanelLeft,
  Pause,
  Play,
  Plus,
  RefreshCcw,
  RotateCcw,
  Search,
  Settings,
  Square,
  SquareCheckBig,
  SkipForward,
  Sun,
  X,
  XCircle,
} from 'lucide-react'
export { DualPaneIcon, SinglePaneIcon } from './custom-icons'

function withDefaults(Icon: LucideIcon) {
  return forwardRef<SVGSVGElement, LucideProps>(function WrappedIcon(props, ref) {
    return <Icon ref={ref} strokeWidth={1.5} {...props} />
  })
}

export const AlertTriangleIcon = withDefaults(AlertTriangle)
export const ArrowLeftIcon = withDefaults(ArrowLeft)
export const ArrowRightIcon = withDefaults(ArrowRight)
export const ArrowUpIcon = withDefaults(ArrowUp)
export const CheckCircleIcon = withDefaults(CheckCircle2)
export const ChevronDownIcon = withDefaults(ChevronDown)
export const ChevronRightIcon = withDefaults(ChevronRight)
export const ChevronUpIcon = withDefaults(ChevronUp)
export const CopyIcon = withDefaults(Copy)
export const DownloadIcon = withDefaults(Download)
export const EyeIcon = withDefaults(Eye)
export const EyeOffIcon = withDefaults(EyeOff)
export const FileIcon = withDefaults(File)
export const FolderIcon = withDefaults(Folder)
export const FolderOpenIcon = withDefaults(FolderOpen)
export const GripVerticalIcon = withDefaults(GripVertical)
export const InfoIcon = withDefaults(Info)
export const LoaderCircleIcon = withDefaults(LoaderCircle)
export const LockIcon = withDefaults(Lock)
export const MoonStarIcon = withDefaults(MoonStar)
export const PanelLeftIcon = withDefaults(PanelLeft)
export const PauseIcon = withDefaults(Pause)
export const PlayIcon = withDefaults(Play)
export const PlusIcon = withDefaults(Plus)
export const RefreshIcon = withDefaults(RefreshCcw)
export const RotateCcwIcon = withDefaults(RotateCcw)
export const SearchIcon = withDefaults(Search)
export const SettingsIcon = withDefaults(Settings)
export const SquareCheckIcon = withDefaults(SquareCheckBig)
export const SquareIcon = withDefaults(Square)
export const SkipForwardIcon = withDefaults(SkipForward)
export const SunIcon = withDefaults(Sun)
export const XIcon = withDefaults(X)
export const XCircleIcon = withDefaults(XCircle)
