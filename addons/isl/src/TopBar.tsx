/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {BookmarksManagerMenu} from './BookmarksManager';
import {BugButton} from './BugButton';
import {BulkActionsMenu} from './BulkActionsMenu';
import serverAPI from './ClientToServerAPI';
import {CwdSelector} from './CwdSelector';
import {DownloadCommitsTooltipButton} from './DownloadCommitsMenu';
import {FocusModeToggle} from './FocusMode';
import {genereatedFileCache} from './GeneratedFile';
import {PullButton} from './PullButton';
import {SettingsGearButton} from './SettingsTooltip';
import {ShelvedChangesMenu} from './ShelvedChanges';
import {DOCUMENTATION_DELAY, Tooltip} from './Tooltip';
import {tracker} from './analytics';
import {Button} from './components/Button';
import {DebugToolsButton} from './debug/DebugToolsButton';
import {t} from './i18n';
import {maybeRemoveForgottenOperation, useClearAllOptimisticState} from './operationsState';
import {haveCommitsLoadedYet, haveRemotePath, isFetchingCommits} from './serverAPIState';
import {useAtomValue} from 'jotai';
import {Icon} from 'shared/Icon';
import {clearTrackedCache} from 'shared/LRU';

import './TopBar.css';

export function TopBar() {
  const loaded = useAtomValue(haveCommitsLoadedYet);
  const canPush = useAtomValue(haveRemotePath);
  if (!loaded) {
    return null;
  }
  return (
    <div className="top-bar">
      <span className="button-group">
        {canPush && <PullButton />}
        <CwdSelector />
        <DownloadCommitsTooltipButton />
        <ShelvedChangesMenu />
        <BulkActionsMenu />
        <BookmarksManagerMenu />
        <FetchingDataIndicator />
      </span>
      <span className="button-group">
        <DebugToolsButton />
        <FocusModeToggle />
        <BugButton />
        <SettingsGearButton />
        <RefreshButton />
      </span>
    </div>
  );
}

function FetchingDataIndicator() {
  const isFetching = useAtomValue(isFetchingCommits);
  return isFetching ? <Icon icon="loading" /> : null;
}

function RefreshButton() {
  const clearOptimisticState = useClearAllOptimisticState();
  return (
    <Tooltip
      delayMs={DOCUMENTATION_DELAY}
      placement="bottom"
      title={t('Re-fetch latest commits and uncommitted changes.')}>
      <Button
        onClick={() => {
          tracker.track('ClickedRefresh');
          clearOptimisticState();
          maybeRemoveForgottenOperation();
          genereatedFileCache.clear(); // allow generated files to be rechecked
          serverAPI.postMessage({type: 'refresh'});
          clearTrackedCache();
        }}
        data-testid="refresh-button">
        <Icon icon="refresh" />
      </Button>
    </Tooltip>
  );
}
