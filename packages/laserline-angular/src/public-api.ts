/**
 * Angular components for the laserline emulator.
 *
 * Setup: `provideLaserline({ wasmUrl, createWorker })`, then use `<ll-laser-view>`,
 * `<ll-controls>` and `<ll-slicer>`, or build custom layouts from the smaller pieces.
 * {@link LaserlineStore} holds the state and every action, including the window layout
 * and `dispatch()` for the shared keyboard shortcuts.
 *
 * @packageDocumentation
 */
export { type LaserlineConfig, LASERLINE_CONFIG } from './lib/config';
export { provideLaserline } from './lib/provide';
export { LaserlineStore, type ControlsPlacement, type SlicerState } from './lib/store';
export { LlLaserView, LlLaserCanvas, LlMeshView, LlRangeImage, LlProfilePlot } from './lib/views';
export { LlControls, LlCameraField, LlTransport, LlPresets, LlReadout, LlObjectScan, LlScanSummary, LlProfileStepper } from './lib/panels';
export { LlSlicer, LlSliceSettings } from './lib/slicer';
export { LlWindowPortal } from './lib/window-portal';
export {
  LlLineControls,
  LlProfileControls,
  LlColorControls,
  LlMotionControls,
  LlDeformControls,
  LlNoiseControls,
  LlScanParamsControls,
  cssRgb,
  type LineParamsValue,
  type ProfileParamsValue,
  type ColorParamsValue,
  type MotionParamsValue,
  type DeformParamsValue,
  type NoiseParamsValue,
  type ScanParamsValue,
  type DisplayMappingValue,
} from './lib/params';
export { LlSlider, LlChoice, LlSection, LlPlot, LlFileButton, THEME, toPosition, fromPosition, type Choice } from './lib/widgets';
