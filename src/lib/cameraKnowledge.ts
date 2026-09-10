export type CameraHotspot = { label: string; x: number; y: number; radius: number };
export type CameraKnowledgePoint = { title: string; plainMeaning: string };
export type CameraPainPoint = { customerNeed: string; concern: string; responseFocus: string };
export type CameraSellingPoint = { title: string; parameterBasis: string; customerBenefit: string };

export type BeginnerCameraLesson = {
  id: string;
  model: string;
  aliases: string;
  image: string;
  imageAlt: string;
  facts: Array<{ label: '焦段' | '有无闪光灯' | '像素' | '适用人群'; value: string }>;
  hotspot: CameraHotspot;
  cardSource: string;
  officialSourceUrl: string;
  knowledgePoints: CameraKnowledgePoint[];
  painPoints: CameraPainPoint[];
  sellingPoints: CameraSellingPoint[];
  boundaries: string[];
  livePitch: string;
  partsDiagramUrl?: string;
  partsSourceUrl?: string;
};

const focalLength = { label: '焦段' as const, value: '机身没有固定焦段，焦段由所配镜头决定' };
const facts = (flash: boolean, pixels: string, audience: string): BeginnerCameraLesson['facts'] => [
  focalLength,
  { label: '有无闪光灯', value: flash ? '有内置闪光灯' : '没有内置闪光灯' },
  { label: '像素', value: pixels },
  { label: '适用人群', value: audience },
];
const image = (name: string) => `/training/camera-cards/${name}`;
const card = (name: string) => `内部参数手卡：${name}`;

export const beginnerCameraLessons: BeginnerCameraLesson[] = [
  {
    id: 'canon-r50', model: '佳能 EOS R50', aliases: 'R50', image: image('canon-r50.png'),
    imageAlt: '佳能 EOS R50 参数卡原图，正面略俯视，可看到机身顶部快门按钮',
    facts: facts(true, '约2420万有效像素', '第一次买相机，主要拍日常、旅行、孩子和短视频的用户'),
    hotspot: { label: '快门按钮', x: 32.5, y: 38.5, radius: 7 }, cardSource: card('佳能 EOS R50手册'),
    officialSourceUrl: 'https://www.canon.com.cn/product/r50/spec.html',
    knowledgePoints: [
      { title: '轻便的 APS-C 入门机', plainMeaning: '约2420万像素，机身连电池和存储卡约375克，外出携带压力较小。' },
      { title: '侧向展开的旋转屏', plainMeaning: '屏幕可转向拍摄者，自拍、低角度取景和竖屏内容更方便。' },
      { title: '没有机身防抖', plainMeaning: '手持视频和暗光拍摄还要看镜头防抖、快门速度和拍摄姿势。' },
    ],
    painPoints: [
      { customerNeed: '第一次买相机，怕不会操作', concern: '不知道该看哪些参数，也怕对焦跟不上孩子。', responseFocus: '先问主要拍什么，再说明自动模式、人物和动物检测能降低上手难度。' },
      { customerNeed: '旅行、自拍和短视频', concern: '担心机身太重，自拍时看不到构图。', responseFocus: '说明约375克机身和侧向旋转屏的作用，同时确认是否经常边走边拍视频。' },
    ],
    sellingPoints: [
      { title: '新手容易开始拍', parameterBasis: 'DIGIC X 处理器、人物与动物检测自动对焦', customerBenefit: '拍家人、孩子和宠物时，减少反复手动选对焦点的负担。' },
      { title: '适合随身记录', parameterBasis: '约375克机身、侧向旋转触控屏', customerBenefit: '旅行携带更轻松，自拍和短视频构图更直观。' },
    ],
    boundaries: ['机身没有五轴防抖，经常手持走动拍视频时还要结合镜头、防抖配件和拍摄方式。', '是否带镜头、配件和具体成色属于当前二手机器信息，需回到本场商品资料。'],
    livePitch: '如果您是第一次买，主要拍孩子、旅行和日常短视频，R50的优势是轻、旋转屏方便、自动对焦比较省心。您更偏照片还是视频，我再按用途帮您缩小镜头方向。',
    partsDiagramUrl: 'https://cam.start.canon/zh/C011/manual/html/screens/UG-00_i0160.svg',
    partsSourceUrl: 'https://cam.start.canon/zh/C011/manual/html/UG-00_Before_0110.html',
  },
  {
    id: 'canon-r6ii', model: '佳能 EOS R6 Mark II', aliases: 'R6二代、R6 II、R62', image: image('canon-r6-mark-ii.png'),
    imageAlt: '佳能 EOS R6 Mark II 参数卡原图',
    facts: facts(false, '约2420万有效像素', '兼顾照片和视频，常拍孩子、宠物、运动或活动记录的用户'),
    hotspot: { label: '快门按钮', x: 29.5, y: 44, radius: 6 }, cardSource: card('佳能 EOS R6 II手册'),
    officialSourceUrl: 'https://www.canon.com.cn/product/r6ii/',
    knowledgePoints: [
      { title: '全画幅照片视频双修', plainMeaning: '约2420万像素全画幅，支持6K超采样4K 60P，照片和视频都能覆盖。' },
      { title: '高速连拍有条件', plainMeaning: '电子快门最高约40张/秒，机械快门最高约12张/秒；实际速度会受镜头、光线和设置影响。' },
      { title: '机身五轴防抖', plainMeaning: '搭配部分RF镜头可协同防抖，适合手持静态照片；运动主体仍要保证快门速度。' },
    ],
    painPoints: [
      { customerNeed: '拍奔跑的孩子、宠物或运动', concern: '担心对焦跟不上，按下快门已经错过动作。', responseFocus: '讲被摄体检测、高速连拍和预拍摄，再提醒高速电子快门的使用条件。' },
      { customerNeed: '照片和视频都要做', concern: '怕买到明显偏科的机型。', responseFocus: '说明全画幅、4K 60P和机身防抖，再确认视频比例、拍摄时长和是否需要双卡备份。' },
    ],
    sellingPoints: [
      { title: '抓拍反应更快', parameterBasis: '电子快门最高约40张/秒、被摄体检测和RAW预拍摄', customerBenefit: '拍运动和孩子时可提高抓到关键瞬间的机会。' },
      { title: '照片视频覆盖较完整', parameterBasis: '约2420万全画幅、6K超采样4K 60P、五轴机身防抖', customerBenefit: '同一机身可兼顾人像、活动记录和视频创作。' },
    ],
    boundaries: ['电子快门高速连拍在快速运动或部分灯光下可能出现果冻效应或条纹，不能只报最高数字。', '如果顾客只求轻便入门，应继续比较重量、预算和使用频率。'],
    livePitch: '您如果经常拍孩子、宠物或者活动，又想兼顾视频，R6二代的重点是对焦、连拍和机身防抖比较均衡。先确认您更常拍运动还是人像，我再讲最有用的部分。',
  },
  {
    id: 'fuji-xt5', model: '富士 X-T5', aliases: 'XT5、X-T5', image: image('fujifilm-x-t5.png'),
    imageAlt: '富士 X-T5 参数卡原图',
    facts: facts(false, '约4020万有效像素', '重视照片细节、胶片模拟、旅行和街拍的用户'),
    hotspot: { label: '快门按钮', x: 31.5, y: 31.5, radius: 5.5 }, cardSource: card('富士 X-T5手册'),
    officialSourceUrl: 'https://www.fujifilm-x.com/en-us/products/cameras/x-t5/specifications/',
    knowledgePoints: [
      { title: '高像素 APS-C', plainMeaning: '约4020万像素，适合重视照片细节和后期裁切空间的用户。' },
      { title: '三向折叠屏', plainMeaning: '方便腰平和竖拍取景，但屏幕不能转到机身正面，不按自拍旋转屏理解。' },
      { title: '连拍数字要带条件', plainMeaning: '机械快门最高约15张/秒；电子快门最高约20张/秒，并带1.29倍裁切。' },
    ],
    painPoints: [
      { customerNeed: '旅行、街拍和照片直出', concern: '希望少做后期，又想保留细节。', responseFocus: '讲4020万像素、胶片模拟和三向折叠屏，并追问是否需要自拍。' },
      { customerNeed: '风光、人像细节和裁切', concern: '担心APS-C细节不够。', responseFocus: '说明高像素带来的细节和裁切空间，同时解释画质还受镜头、光线和拍摄方法影响。' },
    ],
    sellingPoints: [
      { title: '照片细节空间大', parameterBasis: '约4020万像素 X-Trans CMOS 5 HR 传感器', customerBenefit: '风光、静物和人像可保留更多细节，也便于适度裁切。' },
      { title: '手持照片更从容', parameterBasis: '最高约7级五轴机身防抖、三向折叠屏', customerBenefit: '低机位、竖拍和较慢快门的静态题材更方便取景。' },
    ],
    boundaries: ['屏幕不能朝向机身正面，自拍Vlog用户应重点比较旋转屏机型。', '高像素会增加文件体积，对存储卡、电脑和镜头表现也有更高要求。'],
    livePitch: 'X-T5更偏重照片体验，4020万像素、机身防抖和胶片模拟适合旅行、街拍和人像。您如果经常自拍录视频，我会先提醒它是三向折叠屏，不是正面自拍屏。',
  },
  {
    id: 'sony-a7m3', model: '索尼 α7 III', aliases: 'A7M3、7M3、α7 III', image: image('sony-a7-iii.png'),
    imageAlt: '索尼 α7 III 参数卡原图',
    facts: facts(false, '约2420万有效像素', '想进入全画幅并兼顾照片、活动记录和基础视频的用户'),
    hotspot: { label: '快门按钮', x: 22.5, y: 33, radius: 5.5 }, cardSource: card('索尼A7 III手册'),
    officialSourceUrl: 'https://www.sony.com/electronics/support/e-mount-body-ilce-7-series/ilce-7m3/specifications',
    knowledgePoints: [
      { title: '成熟的全画幅基础', plainMeaning: '约2420万像素全画幅，照片、4K视频和暗光记录都有较均衡的基础。' },
      { title: '五轴机身防抖', plainMeaning: '可减轻手持拍摄中的机身抖动，但不能代替拍运动主体所需的快门速度。' },
      { title: '屏幕不能向前翻', plainMeaning: '后屏可以倾斜，低角度取景方便；单人自拍录制时不如侧翻屏直观。' },
    ],
    painPoints: [
      { customerNeed: '预算内进入全画幅', concern: '怕老机型不能兼顾照片和视频。', responseFocus: '说明2420万像素、五轴防抖、4K和镜头生态，再确认用户对新一代视频功能的要求。' },
      { customerNeed: '活动、人像或婚礼记录', concern: '担心续航、对焦和备份。', responseFocus: '讲相位检测对焦、双卡槽和电池表现，同时强调二手机况与卡槽状态要以检测为准。' },
    ],
    sellingPoints: [
      { title: '全画幅用途覆盖广', parameterBasis: '约2420万像素全画幅传感器、4K视频', customerBenefit: '可作为人像、旅行、活动记录和基础视频的同一台主力机。' },
      { title: '手持和活动记录更稳妥', parameterBasis: '五轴机身防抖、693点相位检测、双卡槽', customerBenefit: '日常手持更稳定，重要活动可使用双卡记录降低单卡风险。' },
    ],
    boundaries: ['屏幕不能转向机身正面，单人自拍录制前应考虑外接监看或其他机型。', '作为较早一代机型，若顾客重视4K高帧率、全触控或新一代识别对焦，应继续比较A7 IV和A7C II。'],
    livePitch: 'A7M3适合想用相对成熟预算进入全画幅、照片视频都要兼顾的人。它有机身防抖和双卡槽，但自拍屏和新一代视频功能不是它的重点，您主要拍人像还是活动？',
  },
  {
    id: 'canon-r8', model: '佳能 EOS R8', aliases: 'R8', image: image('canon-r8.png'),
    imageAlt: '佳能 EOS R8 参数卡原图，正面略俯视，可看到手柄顶部快门按钮',
    facts: facts(false, '约2420万有效像素', '希望机身轻便，主要拍旅行、人像并兼顾视频的全画幅用户'),
    hotspot: { label: '快门按钮', x: 23, y: 26.5, radius: 6 }, cardSource: card('佳能 EOS R8手册'),
    officialSourceUrl: 'https://www.canon.com.cn/product/r8/index.html',
    knowledgePoints: [
      { title: '轻量全画幅', plainMeaning: '机身连电池和存储卡约461克，适合重视出行重量的人。' },
      { title: '视频能力来自R6二代同代平台', plainMeaning: '支持6K超采样4K 60P和全高清180P，适合照片视频兼顾。' },
      { title: '没有机身防抖', plainMeaning: '轻便换来部分配置取舍，暗光手持和移动视频要结合镜头与拍摄方式。' },
    ],
    painPoints: [
      { customerNeed: '想要全画幅又不想背重机身', concern: '担心全画幅都很笨重。', responseFocus: '用约461克说明轻便优势，再把镜头重量一起纳入选择。' },
      { customerNeed: '旅行、人像和短视频', concern: '怕轻便机牺牲太多视频能力。', responseFocus: '说明4K 60P、旋转屏和对焦能力，再提醒没有机身防抖。' },
    ],
    sellingPoints: [
      { title: '全画幅与轻便兼顾', parameterBasis: '约2420万像素全画幅、约461克机身', customerBenefit: '旅行和日常携带更轻松，也保留全画幅的画面基础。' },
      { title: '适合混合创作', parameterBasis: '6K超采样4K 60P、侧向旋转屏、被摄体检测', customerBenefit: '人像、旅行照片和短视频可在同一机身上完成。' },
    ],
    boundaries: ['机身没有五轴防抖，长时间手持或走动视频需考虑带防抖镜头、稳定器或其他机型。', '单卡槽与较小电池更适合轻便使用，长时间重要活动要评估续航和备份需求。'],
    livePitch: 'R8适合想要轻便全画幅的人，旅行、人像和短视频都能兼顾。它的机身约461克，但没有机身防抖，您如果经常边走边拍视频，这一点要先说清楚。',
  },
  {
    id: 'canon-r7', model: '佳能 EOS R7', aliases: 'R7', image: image('canon-r7.png'),
    imageAlt: '佳能 EOS R7 参数卡原图，正面可看到手柄顶部快门按钮',
    facts: facts(false, '约3250万有效像素', '常拍鸟类、宠物、运动、孩子或需要远摄裁切的用户'),
    hotspot: { label: '快门按钮', x: 22.6, y: 22.7, radius: 5.5 }, cardSource: card('佳能 EOS R7手册'),
    officialSourceUrl: 'https://www.canon.com.cn/product/r7/spec.html',
    knowledgePoints: [
      { title: '高像素 APS-C', plainMeaning: '约3250万像素，远处主体可以获得较窄视角，也保留一定裁切空间。' },
      { title: '高速连拍与识别对焦', plainMeaning: '机械快门最高约15张/秒，电子快门最高约30张/秒，适合练习抓拍。' },
      { title: '五轴机身防抖和双卡槽', plainMeaning: '手持静态拍摄更方便，重要拍摄也可设置双卡记录。' },
    ],
    painPoints: [
      { customerNeed: '拍鸟、运动或远处的孩子', concern: '主体小、动作快，容易错过。', responseFocus: '解释APS-C视角、高像素、连拍和被摄体检测如何配合，并追问镜头和距离。' },
      { customerNeed: '照片为主，偶尔拍视频', concern: '担心半画幅不够用。', responseFocus: '结合题材说明R7的远摄和速度优势，同时说明画幅不是单独决定画质的因素。' },
    ],
    sellingPoints: [
      { title: '适合远摄与快速主体', parameterBasis: '约3250万像素APS-C、机械最高约15张/秒、电子最高约30张/秒', customerBenefit: '拍鸟、宠物和运动时，更容易把主体拍得更大并连续捕捉动作。' },
      { title: '主力机配置更完整', parameterBasis: '五轴机身防抖、双卡槽、4K 60P', customerBenefit: '照片和视频都能使用，也适合对备份有需求的活动记录。' },
    ],
    boundaries: ['APS-C不是全画幅；顾客重视超广角、浅景深或高感时，应结合镜头和具体场景比较。', '远摄效果仍取决于镜头，不能把机身画幅直接说成免费增加镜头焦距。'],
    livePitch: '您如果主要拍鸟、宠物、运动或者远处的孩子，R7的3250万像素、连拍和识别对焦更对题。先告诉我拍摄距离和准备配什么镜头，机身优势才能真正用出来。',
  },
  {
    id: 'canon-r10', model: '佳能 EOS R10', aliases: 'R10', image: image('canon-r10.png'),
    imageAlt: '佳能 EOS R10 参数卡原图，机身与镜头分开放置，正面可看到快门按钮',
    facts: facts(true, '约2420万有效像素', '想轻便入门，同时学习手动操作和拍摄孩子、宠物的用户'),
    hotspot: { label: '快门按钮', x: 18.8, y: 32.5, radius: 5.5 }, cardSource: card('佳能 EOS R10手册'),
    officialSourceUrl: 'https://www.canon.com.cn/product/r10/spec.html',
    knowledgePoints: [
      { title: '轻便但保留操控', plainMeaning: '约2420万像素APS-C，机身连电池和存储卡约429克，带电子取景器和模式拨盘。' },
      { title: '有内置闪光灯', plainMeaning: '室内应急补光方便，但闪光效果仍受距离、环境和设置影响。' },
      { title: '没有机身防抖', plainMeaning: '拍视频或暗光静态照片时，要结合镜头防抖、快门速度和持机方法。' },
    ],
    painPoints: [
      { customerNeed: '新手想学摄影，又不想一步买太重', concern: '怕入门机很快不够用。', responseFocus: '讲取景器、拨盘、约15张/秒机械连拍和可换镜头，再确认学习意愿。' },
      { customerNeed: '拍孩子或宠物', concern: '动作快，对焦容易丢。', responseFocus: '说明人物、动物检测与高速连拍，同时提醒镜头和光线会影响实际效果。' },
    ],
    sellingPoints: [
      { title: '入门与练习抓拍兼顾', parameterBasis: '人物与动物检测、机械快门最高约15张/秒', customerBenefit: '新手可从自动模式开始，也能逐步练习运动抓拍。' },
      { title: '携带和应急补光方便', parameterBasis: '约429克机身、内置闪光灯', customerBenefit: '旅行和家庭记录负担较小，室内近距离可快速补光。' },
    ],
    boundaries: ['机身没有五轴防抖，稳定性需求较高时需结合镜头或其他设备。', '4K 60P带裁切，讲视频能力时要区分4K 30P和4K 60P。'],
    livePitch: 'R10适合想轻便入门、又希望学一点操控和抓拍的人。它有内置闪光灯，机械连拍最高约15张每秒，但没有机身防抖。您主要拍孩子、宠物还是旅行？',
  },
  {
    id: 'fuji-xs20', model: '富士 X-S20', aliases: 'XS20、X-S20', image: image('fujifilm-x-s20.png'),
    imageAlt: '富士 X-S20 参数卡原图，正面略俯视，可看到快门按钮与电源环',
    facts: facts(true, '约2610万有效像素', '需要轻便机身兼顾照片、视频和自拍Vlog的用户'),
    hotspot: { label: '快门按钮', x: 24.2, y: 35, radius: 5.5 }, cardSource: card('富士 X-S20'),
    officialSourceUrl: 'https://www.fujifilm-x.com/en-us/products/cameras/x-s20/specifications/',
    knowledgePoints: [
      { title: '照片视频兼顾', plainMeaning: '约2610万像素，支持6.2K 30P和4K 60P，覆盖日常照片与进阶视频。' },
      { title: '机身防抖与旋转屏', plainMeaning: '五轴机身防抖最高约7级，侧向旋转屏方便自拍视频构图。' },
      { title: '机身约491克', plainMeaning: '比部分高性能机身轻，但实际出行重量还要加上镜头。' },
    ],
    painPoints: [
      { customerNeed: '一台机身同时拍照片和Vlog', concern: '怕视频方便但照片操作太弱。', responseFocus: '讲旋转屏、防抖、6.2K视频和胶片模拟，再问照片视频比例。' },
      { customerNeed: '手持视频和旅行', concern: '担心画面抖、续航不够。', responseFocus: '说明机身防抖与较大容量电池，同时提醒走动视频仍受步伐和镜头影响。' },
    ],
    sellingPoints: [
      { title: '自拍视频更顺手', parameterBasis: '侧向旋转屏、五轴机身防抖、Vlog模式', customerBenefit: '单人拍摄时更容易确认构图和常用视频设置。' },
      { title: '直出与视频规格兼顾', parameterBasis: '胶片模拟、6.2K 30P、4K 60P', customerBenefit: '可减少部分照片调色工作，也给视频创作保留更多选择。' },
    ],
    boundaries: ['只有一个存储卡槽，重要活动若要求机内双卡同步备份，应比较其他机型。', '防抖不能保证走动视频完全平稳，仍需正确持机或稳定设备。'],
    livePitch: 'X-S20比较适合照片和视频都想拍的人，旋转屏、机身防抖和Vlog操作对单人创作更友好。它是单卡槽，您如果拍婚礼这类重要活动，要先说清备份需求。',
  },
  {
    id: 'fuji-xt30ii', model: '富士 X-T30 II', aliases: 'XT30二代、X-T30 II', image: image('fujifilm-x-t30-ii.png'),
    imageAlt: '富士 X-T30 II 内部参数卡所用机身原图，正面可看到快门按钮与电源环',
    facts: facts(true, '约2610万有效像素', '喜欢轻便复古外形、照片直出、旅行和街拍的入门用户'),
    hotspot: { label: '快门按钮', x: 31, y: 35.3, radius: 5.5 }, cardSource: card('富士 X-T30 II手册'),
    officialSourceUrl: 'https://www.fujifilm-x.com/en-us/products/cameras/x-t30-ii/',
    knowledgePoints: [
      { title: '轻巧的复古拨盘机身', plainMeaning: '约2610万像素APS-C，机身连电池和存储卡约378克，适合随身照片记录。' },
      { title: '倾斜屏不能朝前', plainMeaning: '低角度和腰平取景方便，但单人自拍时看不到正面构图。' },
      { title: '没有机身防抖', plainMeaning: '暗光照片和手持视频需要结合镜头防抖、快门速度与持机方式。' },
    ],
    painPoints: [
      { customerNeed: '喜欢富士色彩又想轻便入门', concern: '怕随身相机太重，直出还要复杂调色。', responseFocus: '讲约378克、胶片模拟和实体拨盘，再确认是否愿意学习拨盘。' },
      { customerNeed: '旅行、街拍和日常照片', concern: '担心屏幕和防抖限制。', responseFocus: '说明倾斜屏适合低角度取景，同时坦白没有机身防抖和正面自拍屏。' },
    ],
    sellingPoints: [
      { title: '适合随身拍照片', parameterBasis: '约378克机身、约2610万像素、实体拨盘', customerBenefit: '旅行和街拍携带轻，曝光设置也能通过拨盘逐步学习。' },
      { title: '照片直出选择多', parameterBasis: '富士胶片模拟、X-Trans CMOS 4', customerBenefit: '新手可先从机内风格开始，减少部分后期调色负担。' },
    ],
    boundaries: ['倾斜屏不能面向拍摄者，自拍Vlog需求强时应比较X-S20等旋转屏机型。', '没有机身防抖，不能只凭“轻便”推断手持视频会更稳定。'],
    livePitch: 'X-T30二代更适合喜欢复古拨盘、照片直出和轻便街拍的人，机身约378克。它没有机身防抖，屏幕也不能转到正面，自拍视频需求强的话要比较别的机型。',
  },
  {
    id: 'fuji-xt30iii', model: '富士 X-T30 III', aliases: 'XT30三代、X-T30 III', image: image('fujifilm-x-t30-iii.png'),
    imageAlt: '富士 X-T30 III 内部参数卡所用机身原图，正面可看到快门按钮与电源环',
    facts: facts(true, '约2610万有效像素', '喜欢轻便复古机身、胶片模拟、日常照片和短视频的用户'),
    hotspot: { label: '快门按钮', x: 32.5, y: 31.7, radius: 5.5 }, cardSource: card('富士 X-T30 III手册'),
    officialSourceUrl: 'https://www.fujifilm-x.com/global/products/cameras/x-t30-iii/',
    knowledgePoints: [
      { title: '轻巧机身加新处理器', plainMeaning: '约2610万像素APS-C，使用X-Processor 5，机身连电池和存储卡约378克。' },
      { title: '胶片模拟拨盘', plainMeaning: '可快速切换机内色彩风格，适合先学会“选风格”，再学习更细的参数。' },
      { title: '视频规格提高但没有机身防抖', plainMeaning: '支持6.2K 30P与4K 60P，走动拍摄仍要结合数字防抖、镜头或稳定器。' },
    ],
    painPoints: [
      { customerNeed: '喜欢直出色彩，不想先学复杂后期', concern: '怕菜单多、风格不好选。', responseFocus: '演示胶片模拟拨盘，再问偏人像、日常还是复古风。' },
      { customerNeed: '轻便旅行并偶尔拍高规格视频', concern: '担心轻机身视频规格不足。', responseFocus: '说明6.2K 30P和4K 60P，同时提醒没有机身防抖。' },
    ],
    sellingPoints: [
      { title: '直出风格更容易选择', parameterBasis: '胶片模拟专用拨盘、20种胶片模拟', customerBenefit: '新人可快速比较色彩风格，减少在菜单里寻找设置的时间。' },
      { title: '轻便机身也能拍高规格视频', parameterBasis: '约378克机身、6.2K 30P、4K 60P', customerBenefit: '旅行随身携带压力较小，同时保留更丰富的视频格式。' },
    ],
    boundaries: ['没有机身防抖，手持走动视频不能只依赖机内数字防抖。', '倾斜屏不能像侧翻屏一样正面监看，自拍需求要单独确认。'],
    livePitch: 'X-T30三代适合喜欢富士直出、想要轻便机身的人，胶片模拟拨盘对新手很直观，视频也支持6.2K 30P。它没有机身防抖，自拍屏需求也要先确认。',
  },
  {
    id: 'sony-a7m4', model: '索尼 α7 IV', aliases: 'A7M4、7M4、α7 IV', image: image('sony-a7-iv.png'),
    imageAlt: '索尼 α7 IV 参数卡原图，正面略俯视，可看到手柄顶部快门按钮',
    facts: facts(false, '约3300万有效像素', '对照片画质、对焦和视频双修都有较高要求的用户'),
    hotspot: { label: '快门按钮', x: 24.8, y: 37.1, radius: 5.5 }, cardSource: card('索尼ILCE-7M4'),
    officialSourceUrl: 'https://www.sony.com/electronics/support/e-mount-body-ilce-7-series/ilce-7m4/specifications',
    knowledgePoints: [
      { title: '约3300万像素全画幅', plainMeaning: '比约2400万像素机型有更多照片细节和裁切空间，也会产生更大的文件。' },
      { title: '照片视频双修定位', plainMeaning: '有五轴机身防抖、侧翻屏和10-bit视频，适合需要兼顾两种工作的用户。' },
      { title: '4K 60P有裁切', plainMeaning: '4K 60P使用Super 35mm画面范围，直播介绍时不能只报“支持4K 60P”。' },
    ],
    painPoints: [
      { customerNeed: '工作和内容创作都要用', concern: '怕照片或视频一边明显妥协。', responseFocus: '讲3300万像素、识别对焦、五轴防抖和10-bit视频，再问工作题材。' },
      { customerNeed: '人像、活动或短视频主力机', concern: '担心可靠性和备份。', responseFocus: '说明双卡槽与照片视频能力，同时回到当前二手机器的检测和卡槽状态。' },
    ],
    sellingPoints: [
      { title: '细节和双修较均衡', parameterBasis: '约3300万像素全画幅、10-bit 4:2:2视频', customerBenefit: '照片有更大裁切空间，视频也保留更多调色余地。' },
      { title: '主体识别范围更广', parameterBasis: '759点相位检测与人物、动物、鸟类识别对焦', customerBenefit: '拍人像、宠物和活动时更容易持续跟住主体。' },
    ],
    boundaries: ['4K 60P存在Super 35mm裁切，广角视频用户要把画面范围算进去。', '约658克机身加全画幅镜头后并不算轻，旅行用户应把整套重量纳入比较。'],
    livePitch: 'A7M4适合照片和视频都要求比较高的人，3300万像素、识别对焦和10-bit视频比较均衡。它的4K 60P有Super 35裁切，旅行使用也要看整套镜头重量。',
  },
  {
    id: 'sony-a7c2', model: '索尼 α7C II', aliases: 'A7C2、7C2、α7C II', image: image('sony-a7c-ii.png'),
    imageAlt: '索尼 α7C II 参数卡原图，正面可看到紧凑机身和手柄',
    facts: facts(false, '约3300万有效像素', '重视轻便，常拍旅行、美食、家庭和日常视频的全画幅用户'),
    hotspot: { label: '快门按钮', x: 22.5, y: 30.5, radius: 6 }, cardSource: card('索尼ILCE-7CM2'),
    officialSourceUrl: 'https://www.sony.com/electronics/support/e-mount-body-ilce-7-series/ilce-7cm2/specifications',
    knowledgePoints: [
      { title: '紧凑的3300万像素全画幅', plainMeaning: '机身连电池和存储卡约514克，适合重视体积的旅行和日常用户。' },
      { title: '识别对焦和五轴防抖', plainMeaning: 'AI处理单元可识别多类主体，五轴防抖最高约7级，手持照片更方便。' },
      { title: '单卡槽设计', plainMeaning: '日常使用更简洁；需要机内双卡同步备份的工作拍摄应比较A7 IV等机型。' },
    ],
    painPoints: [
      { customerNeed: '旅行想要全画幅但怕重', concern: '担心画质和便携只能选一个。', responseFocus: '用约3300万像素、约514克和侧翻屏说明取舍，再把镜头重量一起算。' },
      { customerNeed: '家庭、宠物和单人视频', concern: '怕主体移动时对焦不稳。', responseFocus: '讲多主体识别、五轴防抖和旋转屏，同时追问是否需要双卡备份。' },
    ],
    sellingPoints: [
      { title: '全画幅与紧凑体积兼顾', parameterBasis: '约3300万像素全画幅、约514克机身', customerBenefit: '旅行和日常携带更方便，也保留照片细节和裁切空间。' },
      { title: '单人创作更方便', parameterBasis: '侧向旋转屏、AI主体识别、五轴机身防抖', customerBenefit: '自拍构图、家庭记录和宠物跟焦更容易操作。' },
    ],
    boundaries: ['只有一个存储卡槽，需要机内双卡同步备份的工作应优先比较其他机型。', '紧凑的是机身，搭配大镜头后整套仍可能较重；握持感也应结合镜头和个人手型。'],
    livePitch: 'A7C二代适合想要3300万像素全画幅、又重视旅行体积的人，识别对焦、旋转屏和机身防抖对家庭与单人创作都方便。它是单卡槽，工作备份需求要提前确认。',
  },
];

export function evaluateHotspot(point: { x: number; y: number }, hotspot: CameraHotspot): boolean {
  return Math.hypot(point.x - hotspot.x, point.y - hotspot.y) <= hotspot.radius;
}
