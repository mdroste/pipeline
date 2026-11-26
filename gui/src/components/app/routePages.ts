/** App shell entry points. Every feature remains a dynamic import. */
import { lazy } from "react";
export const ReviewRunView = lazy(() => import("../ReviewRunView"));
export const SettingsPage = lazy(() => import("../SettingsPage"));
export const HomePage = lazy(() => import("../HomePage"));
export const TasksPage = lazy(() => import("../TasksPage"));
export const ProjectIndexPage = lazy(() => import("../ProjectIndexPage"));
export const AppSearch = lazy(() => import("../AppSearch"));
export const WorkspacePage = lazy(() => import("../WorkspacePage"));
export const PipelinePage = lazy(() => import("../PipelinePage"));
export const ResearchActivity = lazy(() => import("../ResearchActivity"));
export const AboutPage = lazy(() => import("../AboutPage"));
export const HistoryPage = lazy(() => import("../HistoryPage"));
export const ProjectsPage = lazy(() => import("../ProjectsPage"));
export const WorkflowGalleryPage = lazy(() => import("../WorkflowGalleryPage"));
export const loadBatchPanel = () => import("../BatchPanel");
export const BatchPanel = lazy(loadBatchPanel);
export const ReportWorkspace = lazy(() => import("../ReportWorkspace"));
