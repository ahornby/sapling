/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {Deferred} from 'shared/utils';

import {Button} from 'isl-components/Button';
import {Icon} from 'isl-components/Icon';
import {atom, useAtom, useSetAtom} from 'jotai';
import React, {useCallback, useEffect, useRef} from 'react';
import {defer} from 'shared/utils';
import {useCommand} from './ISLShortcuts';
import {Modal} from './Modal';
import {writeAtom} from './jotaiUtils';

import './useModal.css';

type ButtonConfig = {label: string | React.ReactNode; primary?: boolean};
type ModalConfigBase = {
  /** Optional codicon to show next to the title */
  icon?: string;
  title?: React.ReactNode;
  width?: string | number;
  height?: string | number;
  maxWidth?: string | number;
  maxHeight?: string | number;
  dataTestId?: string;
};
type ModalConfig<T> = ModalConfigBase &
  (
    | {
        // hack: using 'confirm' mode requires T to be string.
        // The type inference goes wrong if we try to add this constraint directly to the `buttons` field.
        // By adding the constraint here, we get type checking that T is string in order to use this API.
        type: T extends string ? 'confirm' : T extends ButtonConfig ? 'confirm' : never;
        message: React.ReactNode;
        buttons: ReadonlyArray<T>;
      }
    | {
        type: 'custom';
        component: (props: {returnResultAndDismiss: (data: T) => void}) => React.ReactNode;
      }
  );
type ModalState<T> = {
  config: ModalConfig<T>;
  visible: boolean;
  deferred: Deferred<T | undefined>;
};

const modalState = atom<ModalState<unknown | string> | null>(null);

/** Wrapper around <Modal>, generated by `useModal()` hooks. */
export function ModalContainer() {
  const [modal, setModal] = useAtom(modalState);

  // we expect at most one button is "primary"
  const primaryButtonRef = useRef(null);

  const dismiss = () => {
    if (modal?.visible) {
      modal.deferred.resolve(undefined);
      setModal({...modal, visible: false});
    }
  };

  useCommand('Escape', dismiss);

  // focus primary button on mount
  useEffect(() => {
    if (modal?.visible && primaryButtonRef.current != null) {
      (primaryButtonRef.current as HTMLButtonElement).focus();
    }
  }, [primaryButtonRef, modal?.visible]);

  if (modal?.visible !== true) {
    return null;
  }

  let content;
  if ((modal.config as ModalConfig<string>).type === 'confirm') {
    const config = modal.config as ModalConfig<string> & {type: 'confirm'};
    content = (
      <>
        <div id="use-modal-message">{config.message}</div>
        <div className="use-modal-buttons">
          {config.buttons.map((button: string | ButtonConfig, index: number) => {
            const label = typeof button === 'object' ? button.label : button;
            const isPrimary = typeof button === 'object' && button.primary != null;
            return (
              <Button
                kind={isPrimary ? 'primary' : undefined}
                onClick={() => {
                  modal.deferred.resolve(button);
                  setModal({...modal, visible: false});
                }}
                ref={isPrimary ? primaryButtonRef : undefined}
                key={index}>
                {label}
              </Button>
            );
          })}
        </div>
      </>
    );
  } else if (modal.config.type === 'custom') {
    content = modal.config.component({
      returnResultAndDismiss: data => {
        modal.deferred.resolve(data);
        setModal({...modal, visible: false});
      },
    });
  }

  return (
    <Modal
      height={modal.config.height}
      width={modal.config.width}
      maxHeight={modal.config.maxHeight}
      maxWidth={modal.config.maxWidth}
      className="use-modal"
      aria-labelledby="use-modal-title"
      aria-describedby="use-modal-message"
      dataTestId={modal.config.dataTestId}
      dismiss={dismiss}>
      {modal.config.title != null && (
        <div id="use-modal-title">
          {modal.config.icon != null ? <Icon icon={modal.config.icon} size="M" /> : null}
          {typeof modal.config.title === 'string' ? (
            <span>{modal.config.title}</span>
          ) : (
            modal.config.title
          )}
        </div>
      )}
      {content}
    </Modal>
  );
}

/**
 * Hook that provides a callback to show a modal with customizable behavior.
 * Modal has a dismiss button & dismisses on Escape keypress, thus you must always be able to handle
 * returning `undefined`.
 *
 * For now, we assume all uses of useOptionModal are triggered directly from a user action.
 * If that's not the case, it would be possible to have multiple modals overlap.
 **/
export function useModal(): <T>(config: ModalConfig<T>) => Promise<T | undefined> {
  const setModal = useSetAtom(modalState);

  return useCallback(
    <T,>(config: ModalConfig<T>) => {
      const deferred = defer<T | undefined>();
      // The API we provide is typed with T, but our recoil state only knows `unknown`, so we have to cast.
      // This is safe because only one modal is visible at a time, so we know the data type we created it with is what we'll get back.
      setModal({
        config: config as ModalConfig<unknown>,
        visible: true,
        deferred: deferred as Deferred<unknown | undefined>,
      });

      return deferred.promise as Promise<T>;
    },
    [setModal],
  );
}

export function showModal<T>(config: ModalConfig<T>): Promise<T | undefined> {
  const deferred = defer<T | undefined>();
  writeAtom(modalState, {
    config: config as ModalConfig<unknown>,
    visible: true,
    deferred: deferred as Deferred<unknown | undefined>,
  });

  return deferred.promise as Promise<T>;
}
