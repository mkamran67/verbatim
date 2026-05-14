import { HashRouter, useNavigate, useLocation } from "react-router-dom";
import { AppRoutes } from "./router";
import { I18nextProvider } from "react-i18next";
import i18n from "./i18n";
import { useEffect } from "react";
import { Provider } from "react-redux";
import { store } from "./store";
import { useAppDispatch, useAppSelector } from "./store/hooks";
import { useTauriListeners } from "./store/useTauriListeners";
import { fetchConfig } from "./store/slices/configSlice";
import { fetchStats } from "./store/slices/statsSlice";
import { fetchWhisperModels, fetchLlmModels } from "./store/slices/modelsSlice";
import { loadThemeFromConfig } from "./store/slices/themeSlice";
import { fetchDeepgramBalance } from "./store/slices/balanceSlice";
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
  const onboardingComplete = useAppSelector((s) => s.config.data?.general.onboarding_complete);
  const configLoaded = useAppSelector((s) => s.config.data != null);

  useEffect(() => {
    if (location.pathname === "/onboarding") return;
    if (!configLoaded) return; // wait for fetchConfig to land before deciding

    // First-run / factory-reset: no config or onboarding flag never flipped.
    if (onboardingComplete === false) {
      navigate("/onboarding", { replace: true });
      return;
    }

    let cancelled = false;

    (async () => {
      try {
        const [mac, linuxOk] = await Promise.all([
          api.checkMacPermissions(),
          api.checkLinuxInputPermission(),
        ]);
        if (cancelled) return;

        // Required permissions: macOS = AX + Mic + Input Monitoring;
        // Linux = evdev /dev/input access. Automation is informational only.
        const macRequiredOk =
          !mac || (mac.accessibility && mac.microphone && mac.input_monitoring);
        if (!macRequiredOk || !linuxOk) {
          navigate("/onboarding", { replace: true });
        }
      } catch {
        // If the check itself fails, don't block the app.
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [location.pathname, navigate, onboardingComplete, configLoaded]);

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
