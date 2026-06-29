import { forwardRef } from 'react'
import type { LucideIcon, LucideProps } from 'lucide-react'
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Copy,
  Cpu,
  Database,
  Disc,
  Download,
  Eye,
  EyeOff,
  File,
  FilePlus2,
  FileArchive,
  FileAudio,
  FileCode2,
  FileCog,
  FileImage,
  FileText,
  FileType,
  FileVideo,
  Folder,
  FolderCog,
  FolderDown,
  FolderGit2,
  FolderPlus,
  FolderOpen,
  GripVertical,
  HardDrive,
  Info,
  LoaderCircle,
  Lock,
  MoonStar,
  Network,
  Package,
  PackageOpen,
  PanelLeft,
  Pause,
  Play,
  Plus,
  RefreshCcw,
  Scissors,
  RotateCcw,
  Search,
  Settings,
  Share2,
  Square,
  SquareCheckBig,
  SkipForward,
  Sun,
  Trash2,
  Type,
  Usb,
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
export const CheckIcon = withDefaults(Check)
export const ArrowLeftIcon = withDefaults(ArrowLeft)
export const ArrowRightIcon = withDefaults(ArrowRight)
export const ArrowUpIcon = withDefaults(ArrowUp)
export const CheckCircleIcon = withDefaults(CheckCircle2)
export const ChevronDownIcon = withDefaults(ChevronDown)
export const ChevronRightIcon = withDefaults(ChevronRight)
export const ChevronUpIcon = withDefaults(ChevronUp)
export const CopyIcon = withDefaults(Copy)
export const CpuIcon = withDefaults(Cpu)
export const DatabaseIcon = withDefaults(Database)
export const DiscIcon = withDefaults(Disc)
export const DownloadIcon = withDefaults(Download)
export const EyeIcon = withDefaults(Eye)
export const EyeOffIcon = withDefaults(EyeOff)
export const FileIcon = withDefaults(File)
export const FilePlusIcon = withDefaults(FilePlus2)
export const FileArchiveIcon = withDefaults(FileArchive)
export const FileAudioIcon = withDefaults(FileAudio)
export const FileCode2Icon = withDefaults(FileCode2)
export const FileCogIcon = withDefaults(FileCog)
export const FileImageIcon = withDefaults(FileImage)
export const FileTextIcon = withDefaults(FileText)
export const FileTypeIcon = withDefaults(FileType)
export const FileVideoIcon = withDefaults(FileVideo)
export const FolderIcon = withDefaults(Folder)
export const FolderCogIcon = withDefaults(FolderCog)
export const FolderDownIcon = withDefaults(FolderDown)
export const FolderGit2Icon = withDefaults(FolderGit2)
export const FolderPlusIcon = withDefaults(FolderPlus)
export const FolderOpenIcon = withDefaults(FolderOpen)
export const GripVerticalIcon = withDefaults(GripVertical)
export const HardDriveIcon = withDefaults(HardDrive)
export const InfoIcon = withDefaults(Info)
export const LoaderCircleIcon = withDefaults(LoaderCircle)
export const LockIcon = withDefaults(Lock)
export const MoonStarIcon = withDefaults(MoonStar)
export const NetworkIcon = withDefaults(Network)
export const PackageIcon = withDefaults(Package)
export const PackageOpenIcon = withDefaults(PackageOpen)
export const PanelLeftIcon = withDefaults(PanelLeft)
export const PauseIcon = withDefaults(Pause)
export const PlayIcon = withDefaults(Play)
export const PlusIcon = withDefaults(Plus)
export const RefreshIcon = withDefaults(RefreshCcw)
export const ScissorsIcon = withDefaults(Scissors)
export const RotateCcwIcon = withDefaults(RotateCcw)
export const SearchIcon = withDefaults(Search)
export const SettingsIcon = withDefaults(Settings)
export const Share2Icon = withDefaults(Share2)
export const SquareCheckIcon = withDefaults(SquareCheckBig)
export const SquareIcon = withDefaults(Square)
export const SkipForwardIcon = withDefaults(SkipForward)
export const SunIcon = withDefaults(Sun)
export const Trash2Icon = withDefaults(Trash2)
export const TypeIcon = withDefaults(Type)
export const UsbIcon = withDefaults(Usb)
export const XIcon = withDefaults(X)
export const XCircleIcon = withDefaults(XCircle)
