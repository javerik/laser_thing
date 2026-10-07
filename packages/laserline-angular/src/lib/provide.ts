import type { Provider } from '@angular/core';
import { LASERLINE_CONFIG, type LaserlineConfig } from './config';
import { LaserlineStore } from './store';

/**
 * Provides a `LaserlineStore` shared by the laserline components below the injector
 * (application-wide in `ApplicationConfig.providers`, or per component).
 */
export function provideLaserline(config: LaserlineConfig): Provider[] {
  return [{ provide: LASERLINE_CONFIG, useValue: config }, LaserlineStore];
}
