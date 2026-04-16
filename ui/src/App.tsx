import { HashRouter, useNavigate, useLocation } from "react-router-dom";
import { AppRoutes } from "./router";
import { I18nextProvider } from "react-i18next";
import i18n from "./i18n";
import { useEffect, useRef } from "react";
import { Provider } from "react-redux";
import { store } from "./store";
import { useAppDispatch, useAppSelector } from "./store/hooks";
import { useTauriListeners } from "./store/useTauriListeners";
import { fetchConfig, saveConfig } from "./store/slices/configSlice";
import { fetchStats } from "./store/slices/statsSlice";
import { fetchWhisperModels, fetchLlmModels } from "./store/slices/modelsSlice";
import { loadThemeFromConfig } from "./store/slices/themeSlice";
import { fetchDeepgramBalance, fetchOpenaiCosts } from "./store/slices/balanceSlice";
import { fetchRecent } from "./store/slices/transcriptionsSlice";
import { applyTheme } from "./lib/theme";
import { api } from "./lib/tauri";

function TauriEventBridge() {
  const dispatch = useAppDispatch();

  useTauriListeners();

  useEffect(() => {
    dispatch(fetchConfig()).then((action) => {
      if (fetchConfig.fulfilled.match(action)) {
        const config = action.payload;
        if (config.deepgram.api_key) dispatch(fetchDeepgramBalance(false));
        if (config.openai.admin_key) dispatch(fetchOpenaiCosts(false));
        // Sync UI language from config
        const uiLang = config.general.ui_language;
        if (uiLang && uiLang !== 'system') {
          i18n.changeLanguage(uiLang);
        }
      }
    });
    dispatch(loadThemeFromConfig());
    dispatch(fetchStats());
    dispatch(fetchWhisperModels());
    dispatch(fetchLlmModels());
    dispatch(fetchRecent(50));
  }, [dispatch]);

  return null;
}

function ThemeApplier() {
  const theme = useAppSelector((s) => s.theme.value);

  useEffect(() => {
    applyTheme(theme);

    if (theme === "system") {
      const mq = window.matchMedia("(prefers-color-scheme: dark)");
      const handler = () => applyTheme("system");
      mq.addEventListener("change", handler);
      return () => mq.removeEventListener("change", handler);
    }
  }, [theme]);

  return null;
}

function PermissionGate({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();
  const location = useLocation();
  const dispatch = useAppDispatch();
  const config = useAppSelector((s) => s.config.data);
  const onboardingComplete = useAppSelector(
    (s) => s.config.data?.general.onboarding_complete,
  );
  const configRef = useRef(config);
  configRef.current = config;

  useEffect(() => {
    if (location.pathname === "/onboarding") return;
    if (!configRef.current) return;
    if (onboardingComplete) return;

    let cancelled = false;

    (async () => {
      try {
        const perms = await api.checkMacPermissions();
        if (cancelled) return;
        if (!perms) return;

        const current = configRef.current;
        if (!current) return;

        if (!perms.accessibility || !perms.microphone) {
          navigate("/onboarding", { replace: true });
        } else {
          const updated = structuredClone(current);
          updated.general.onboarding_complete = true;
          dispatch(saveConfig(updated));
        }
      } catch {
        // If check fails, don't block the app
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [location.pathname, navigate, onboardingComplete, dispatch]);

  return <>{children}</>;
}

function App() {
  return (
    <I18nextProvider i18n={i18n}>
      <Provider store={store}>
        <ThemeApplier />
        <HashRouter>
          <TauriEventBridge />
          <PermissionGate>
            <AppRoutes />
          </PermissionGate>
        </HashRouter>
      </Provider>
    </I18nextProvider>
  );
}

export default App;
