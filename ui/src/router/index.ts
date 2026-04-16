import { useRoutes } from "react-router-dom";
import routes from "./config";

export function AppRoutes() {
  return useRoutes(routes);
}
