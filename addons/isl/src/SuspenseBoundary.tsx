/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {ErrorBoundary} from './ErrorNotice';
import {Suspense} from 'react';
import {Icon} from 'shared/Icon';

/**
 * <ErrorBoundary> and <Suspense>, with a default fallback.
 */
export function SuspenseBoundary(props: {
  children: React.ReactNode;
  fallback?: JSX.Element;
}): JSX.Element {
  const fallback = props.fallback ?? <Icon icon="loading" />;

  return (
    <ErrorBoundary>
      <Suspense fallback={fallback}>{props.children}</Suspense>
    </ErrorBoundary>
  );
}
