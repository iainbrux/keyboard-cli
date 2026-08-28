import { HigherKeyController } from './src/controller/higherKey';
import { InfoController } from './src/controller/info';
import { KeyController } from './src/controller/key';
import { LightingController } from './src/controller/lighting';
import { PerformanceController } from './src/controller/performance';
import { SystemController } from './src/controller/system';
declare const keyboardProtocol: {
    higherKeyProtocol: HigherKeyController;
    infoProtocol: InfoController;
    keyProtocol: KeyController;
    lightingProtocol: LightingController;
    systemProtocol: SystemController;
    performanceProtocol: PerformanceController;
};
export * as constantsParam from './src/constants/param';
export * from './src/types/interface';
export default keyboardProtocol;
