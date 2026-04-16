import type { RouteObject } from "react-router-dom";
import NotFound from "../pages/NotFound";
import Home from "../pages/home/page";
import Recordings from "../pages/recordings/page";
import WordCount from "../pages/word-count/page";
import SpeechToText from "../pages/speech-to-text/page";
import PostProcessing from "../pages/post-processing/page";
import Settings from "../pages/settings/page";
import ApiKeys from "../pages/api-keys/page";
import ApiUsage from "../pages/api-usage/page";
import Onboarding from "../pages/onboarding/page";
import About from "../pages/about/page";

const routes: RouteObject[] = [
  {
    path: "/",
    element: <Home />,
  },
  {
    path: "/onboarding",
    element: <Onboarding />,
  },
  {
    path: "/recordings",
    element: <Recordings />,
  },
  {
    path: "/word-count",
    element: <WordCount />,
  },
  {
    path: "/speech-to-text",
    element: <SpeechToText />,
  },
  {
    path: "/post-processing",
    element: <PostProcessing />,
  },
  {
    path: "/settings",
    element: <Settings />,
  },
  {
    path: "/api-keys",
    element: <ApiKeys />,
  },
  {
    path: "/api-usage",
    element: <ApiUsage />,
  },
  {
    path: "/about",
    element: <About />,
  },
  {
    path: "*",
    element: <NotFound />,
  },
];

export default routes;
