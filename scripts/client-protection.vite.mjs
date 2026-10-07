import base from '../vite.config.ts';
import { clientProtectionPlugin } from './client-protection.mjs';
export default {...base,plugins:[...base.plugins,clientProtectionPlugin()],build:{...base.build,sourcemap:false}};
