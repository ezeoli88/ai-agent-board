export type {
  Spec,
  SpecStatus,
  CreateSpecInput,
  UpdateSpecInput,
  ApproveSpecStepInput,
  AnswerClarificationsInput,
  CreateTasksFromSpecResponse,
} from '@dash-agent/shared';

export {
  SPEC_STATUSES,
  SPEC_STATUS_LABELS,
  SPEC_STATUS_COLORS,
  isSpecTerminalStatus,
  getSpecAvailableActions,
} from '@dash-agent/shared';
