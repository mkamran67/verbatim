import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import RecordingInterface from './components/RecordingInterface';
import RecordingsList from './components/RecordingsList';

export default function Recordings() {
  const { t } = useTranslation();
  return (
    <Layout
      title={t('recordings.title')}
      subtitle={t('recordings.subtitle')}
    >
      <div className="flex flex-col gap-5 max-w-[1200px]">
        <RecordingInterface />
        <RecordingsList />
      </div>
    </Layout>
  );
}
