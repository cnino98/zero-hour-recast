#!/usr/bin/env python3
"""Prepare private metadata fixtures from owned INIZH.big and pinned source.

Extract native registration methods from the pinned source and object
declarations from the installation. The resulting fixtures compare module
metadata and registration order.
"""
from pathlib import Path
import argparse, hashlib, json, os, re, struct, subprocess

PIN = "2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--archive", type=Path, required=True, help="Owned INIZH.big")
parser.add_argument("--source", type=Path, required=True, help="Pinned SuperHackers checkout")
parser.add_argument("--output", type=Path, required=True, help="Private output directory outside this repository")
parser.add_argument("--compare", type=Path, help="Existing test-fixtures directory for byte comparison")
parser.add_argument("--cxx", default="clang++")
parser.add_argument("--maps-archive", type=Path, help="Optional owned MapsZH.big for 12 custom-map chooser fixtures")
args = parser.parse_args()
UP = args.source.resolve()
ROOT = UP.parent
out = args.output.resolve()
repo = Path(__file__).resolve().parents[2]
if out == repo or repo in out.parents:
    parser.error("--output must be outside the public repository")
if subprocess.check_output(["git", "-C", str(UP), "rev-parse", "HEAD"], text=True).strip() != PIN:
    parser.error("source checkout does not match the frozen pin")
if subprocess.check_output(["git", "-C", str(UP), "status", "--porcelain", "--untracked-files=normal"], text=True).strip():
    parser.error("source checkout has modifications or untracked files")
if out.exists():
    parser.error("--output must be a new directory; existing files are never overwritten")
out.mkdir(parents=True)
BASE = out / "reference-r2"
# Frozen original case order, including the required GLAHole parent.
FIXED = ['1000DollarCrate', '100DollarCrate', '100DollarCrateForBuildings', '1500DollarCrate', '200DollarCrate', '2500DollarCrate', 'AmericaAirfield', 'AmericaBarracks', 'AmericaCommandCenter', 'AmericaFireBase', 'AmericaInfantryMissileDefender', 'AmericaInfantryPathfinder', 'AmericaInfantryRanger', 'AmericaJetAurora', 'AmericaJetAuroraHulk', 'AmericaJetRaptor', 'AmericaJetRaptorHulk', 'AmericaJetStealthFighter', 'AmericaJetStealthHulk', 'AmericaParachute', 'AmericaPatriotBattery', 'AmericaPowerPlant', 'AmericaStrategyCenter', 'AmericaSupplyCenter', 'AmericaTankAvenger', 'AmericaTankAvengerLaserTurret', 'AmericaTankCrusader', 'AmericaTankMicrowave', 'AmericaTankPaladin', 'AmericaVehicleBattleDrone', 'AmericaVehicleChinook', 'AmericaVehicleComanche', 'AmericaVehicleDozer', 'AmericaVehicleHellfireDrone', 'AmericaVehicleHumvee', 'AmericaVehicleHumveeDeadHull', 'AmericaVehicleMedic', 'AmericaVehicleScoutDrone', 'AmericaVehicleSentryDrone', 'AmericaVehicleSentryDroneHulk', 'AmericaVehicleTomahawk', 'AmericaWarFactory', 'BlackNapalmFirestormSmall', 'CarCompact', 'CarScrap', 'CarSport01', 'CarSport02', 'CarTaxiCab02', 'ChinaAirfield', 'ChinaBarracks', 'ChinaBunker', 'ChinaCommandCenter', 'ChinaGattlingCannon', 'ChinaHelixBattleBunker', 'ChinaHelixGattlingCannon', 'ChinaHelixPropagandaTower', 'ChinaInfantryBlackLotus', 'ChinaInfantryHacker', 'ChinaInfantryRedguard', 'ChinaInfantryTankHunter', 'ChinaInternetCenter', 'ChinaJetMIG', 'ChinaJetMIGHulk', 'ChinaPowerPlant', 'ChinaPropagandaCenter', 'ChinaSpeakerTower', 'ChinaSupplyCenter', 'ChinaTankBattleMaster', 'ChinaTankDragon', 'ChinaTankECM', 'ChinaTankGattling', 'ChinaTankOverlord', 'ChinaTankOverlordBattleBunker', 'ChinaTankOverlordGattlingCannon', 'ChinaTankOverlordPropagandaTower', 'ChinaVehicleDozer', 'ChinaVehicleHelix', 'ChinaVehicleInfernoCannon', 'ChinaVehicleListeningOutpost', 'ChinaVehicleListeningOutpostDeadHull', 'ChinaVehicleSupplyTruck', 'ChinaVehicleTroopCrawler', 'ChinaWarFactory', 'DeadBattleBusHulk', 'DeadCrusaderHulk', 'FakeGLAArmsDealer', 'FakeGLABarracks', 'FakeGLABlackMarket', 'FakeGLACommandCenter', 'FakeGLASupplyStash', 'FireFieldSmall', 'FireFieldUpgradedSmall', 'FireWallSegment', 'FireWallSegmentUpgraded', 'FirestormSmall', 'FlamingInfantry', 'ForkliftSmall', 'GLAArmsDealer', 'GLABarracks', 'GLABlackMarket', 'GLACommandCenter', 'GLADemoTrap', 'GLAHole', 'GLAHoleArmsDealer', 'GLAHoleBarracks', 'GLAHoleBlackMarket', 'GLAHoleCommandCenter', 'GLAHolePalace', 'GLAHoleStingerSite', 'GLAHoleSupplyStash', 'GLAHoleTunnelNetwork', 'GLAInfantryAngryMobMolotov02', 'GLAInfantryAngryMobNexus', 'GLAInfantryAngryMobPistol01', 'GLAInfantryAngryMobPistol03', 'GLAInfantryAngryMobPistol05', 'GLAInfantryAngryMobRock02', 'GLAInfantryAngryMobRock04', 'GLAInfantryHijacker', 'GLAInfantryJarmenKell', 'GLAInfantryRebel', 'GLAInfantrySaboteur', 'GLAInfantryStingerSoldier', 'GLAInfantryTerrorist', 'GLAInfantryTunnelDefender', 'GLAInfantryWorker', 'GLAPalace', 'GLAStingerSite', 'GLASupplyStash', 'GLATankMarauder', 'GLATankScorpion', 'GLATunnelNetwork', 'GLAVehicleBattleBus', 'GLAVehicleQuadCannon', 'GLAVehicleRadarVan', 'GLAVehicleRocketBuggy', 'GLAVehicleScudLauncher', 'GLAVehicleTechnical', 'GLAVehicleTechnicalChassisOne', 'GLAVehicleTechnicalChassisThree', 'GLAVehicleTechnicalChassisTwo', 'GLAVehicleToxinTruck', 'GenericDebris', 'HelixRubbleHull', 'NapalmBomb', 'SalvageCrate', 'SupplyDropZoneCrate', 'TechArtilleryPlatform', 'TechHospital', 'TechOilDerrick', 'TechOilRefinery', 'TechRepairPad', 'TechRepairbay', 'ToxicInfantry', 'ToxicInfantryBeta', 'ToxicInfantryBeta_TerroristOnly', 'ToxicInfantry_TerroristOnly', 'TractorCombine', 'TruckWork']

def rel(path):
    return str(path.relative_to(ROOT))

def clean(text):
    return re.sub(r'/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'',lambda m:''.join('\n' if c=='\n' else ' ' for c in m[0]),text,flags=re.S)
def block(text,start):
    scanned=clean(text);pos=scanned.index('{',start); depth=1; end=pos+1
    while depth and end<len(scanned):
        depth += (scanned[end]=='{')-(scanned[end]=='}'); end+=1
    return text[pos+1:end-1]
def big(path):
    data=path.read_bytes(); assert data[:4] in (b'BIGF',b'BIG4'); cursor=16; result={}
    for _ in range(struct.unpack_from('>I',data,8)[0]):
        off,size=struct.unpack_from('>II',data,cursor);end=data.index(b'\0',cursor+8)
        name=data[cursor+8:end].decode('ascii').replace('\\','/').lower();cursor=end+1
        result[name]=data[off:off+size]
    return result
assets=big(args.archive)
objects={}; weapons={}; hashes={}
for path,data in assets.items():
    if not path.endswith('.ini'): continue
    text=data.decode('latin1'); hashes[path]=hashlib.sha256(data).hexdigest()
    markers=list(re.finditer(r'(?m)^(ObjectReskin|ChildObject|Object|Weapon)\s+(\w+)(?:[^\S\r\n]+(\w+))?',text))
    for i,m in enumerate(markers):
        record={'name':m[2],'kind':m[1],'parent':m[3] if m[1]!='Object' and m[1]!='Weapon' else None,'asset':path,'line':text.count('\n',0,m.start())+1,'raw':text[m.start():markers[i+1].start() if i+1<len(markers) else len(text)]}
        (weapons if m[1]=='Weapon' else objects)[m[2]]=record

classes={}; headers={}; sources={}
for p in (UP/'GeneralsMD/Code/GameEngine/Include').rglob('*.h'):
    text=p.read_text(errors='replace')
    for m in re.finditer(r'\bclass\s+(\w+)\s*:\s*((?:public|protected|private)\s+[^\{;]+)\{',text):
        name=m[1]; body=block(text,m.start());bases=re.findall(r'(?:public|protected|private)\s+(\w+)',m[2]);mask=re.search(r'\bgetInterfaceMask\s*\([^)]*\)\s*\{',body)
        masks=re.findall(r'MODULEINTERFACE_\w+',block(body,mask.start())) if mask else []
        inherited=re.findall(r'(\w+)::getInterfaceMask',block(body,mask.start())) if mask else bases[:1]
        phase=re.search(r'getUpdatePhase\s*\([^)]*\)[^{]*\{[^}]*return\s+(PHASE_\w+)',body)
        classes[name]={'bases':bases,'mask_literals':masks,'mask_inherited':inherited,'phase':phase[1] if phase else None,'header':rel(p),'line':text.count('\n',0,m.start())+1,'body':body,'body_line':text.count('\n',0,text.index('{',m.start()))+1}

def build_factory_identity(classes, upstream, root, families, block):
    def clean(text):
        return re.sub(r'/\*.*?\*/|//[^\n]*',lambda m:''.join('\n' if char=='\n' else ' ' for char in m[0]),text,flags=re.S)
    # The older inventory indexed only classes; actual factory data also uses
    # structs (for example UpgradeModuleData), with normal C++ inheritance.
    for tree in [upstream/'GeneralsMD/Code/GameEngine/Include', upstream/'Core/GameEngineDevice/Include']:
        for path in tree.rglob('*.h'):
            text=path.read_text(errors='replace')
            for match in re.finditer(r'\b(?:class|struct)\s+(\w+)(?:\s+final)?\s*:\s*([^\{;]+)\{',clean(text)):
                if match[1] in classes:continue
                bases=[]
                for base in match[2].split(','):
                    base=re.sub(r'\b(?:public|protected|private|virtual)\b','',base).strip()
                    if re.fullmatch(r'\w+',base):bases.append(base)
                classes[match[1]]={'bases':bases,'body':block(text,match.start()),'header':str(path.relative_to(root)),'line':text.count('\n',0,match.start())+1}
    module_h=upstream/'GeneralsMD/Code/GameEngine/Include/Common/Module.h'
    module_text=module_h.read_text()
    module_cpp=upstream/'GeneralsMD/Code/GameEngine/Source/Common/Thing/Module.cpp'
    module_cpp_text=module_cpp.read_text()
    definitions={}
    for name,c in classes.items():
        body=clean(c['body'])
        association=re.search(r'MAKE_STANDARD_MODULE_(?:MACRO_WITH_MODULE_DATA|DATA_MACRO_ABC)\s*\(\s*(\w+)\s*,\s*(\w+)\s*\)',body)
        if association:
            if association[1]!=name:raise ValueError(('factory macro owner mismatch',name,association[1]))
            definitions[name]={'owner':name,'produced':association[2],'kind':'macro','source':c['header'],'class_line':c['line'],'association':association[0]}
        inline=re.search(r'(?:static\s+)?ModuleData\s*\*\s*friend_newModuleData\s*\([^)]*\)\s*\{',body)
        if inline:
            if name in definitions:raise ValueError(('ambiguous own factory',name))
            code=body[inline.start():body.index('{',inline.start())+1]+block(body,inline.start())+'}'
            produced=re.search(r'(?:MSGNEW\([^)]*\)\s*|new\s+)(\w+)',code)
            if not produced:raise ValueError(('unresolved explicit factory produced class',name))
            definitions[name]={'owner':name,'produced':produced[1],'kind':'inline','source':c['header'],'class_line':c['line'],'code':code}
    # Explicit Module::friend_newModuleData is declared in Module.h and defined here.
    start=module_cpp_text.index('ModuleData* Module::friend_newModuleData(')
    module_factory=module_cpp_text[start:module_cpp_text.index('\n}',start)+2]
    produced=re.search(r'MSGNEW\([^)]*\)\s*(\w+)',module_factory)
    if not produced:raise ValueError('Module factory allocation unresolved')
    definitions['Module']={'owner':'Module','produced':produced[1],'kind':'explicit','source':str(module_cpp.relative_to(root)),'line':module_cpp_text.count('\n',0,start)+1,'code':module_factory}
    cache={}
    def resolve(name,seen=()):
        if name in cache:return cache[name]
        if name in seen:raise ValueError(('factory ancestry cycle',name,seen))
        if name in definitions:cache[name]=definitions[name];return cache[name]
        c=classes.get(name)
        if not c:return None
        candidates=[r for base in c['bases'] if (r:=resolve(base,(*seen,name))) is not None]
        unique={r['owner']:r for r in candidates}
        if len(unique)>1:raise ValueError(('ambiguous inherited factory',name,sorted(unique)))
        if not unique:return None
        cache[name]=next(iter(unique.values()));return cache[name]
    marker_cache={}
    def marker(name,seen=()):
        if name in marker_cache:return marker_cache[name]
        if name in seen:raise ValueError(('AI marker ancestry cycle',name))
        c=classes.get(name)
        if not c:return None
        body=clean(c['body'])
        own=re.search(r'(?:virtual\s+)?Bool\s+isAiModuleData\s*\([^)]*\)[^{;]*\{',body)
        if own:
            code=body[own.start():body.index('{',own.start())+1]+block(body,own.start())+'}'
            truth=re.search(r'\breturn\s+(true|false)\s*;',code)
            if not truth:raise ValueError(('nonconstant AI identity requires source adaptation',name,code))
            marker_cache[name]={'owner':name,'source':c['header'],'class_line':c['line'],'code':code,'ai':truth[1]=='true'}
            return marker_cache[name]
        inherited={r['owner']:r for base in c['bases'] if (r:=marker(base,(*seen,name))) is not None}
        if len(inherited)>1:raise ValueError(('ambiguous AI marker',name))
        if not inherited:return None
        marker_cache[name]=next(iter(inherited.values()));return marker_cache[name]
    identities={}
    for family in sorted(families):
        factory=resolve(family)
        if factory is None:raise ValueError(('unresolved native data factory',family))
        ai=marker(factory['produced'])
        if ai is None:raise ValueError(('unresolved native virtual AI marker',family,factory['produced']))
        identities[family]={'factory':factory,'marker':ai,'ai':ai['ai']}
    base_marker=marker('ModuleData')
    if base_marker is None:raise ValueError('ModuleData marker missing')
    global_data='struct MultiIniFieldParse {};\nstruct ModuleData { virtual ~ModuleData()=default; '+base_marker['code']+' static void buildFieldParse(MultiIniFieldParse&){} };'
    # Emit produced-data ancestry, retaining actual nearest marker declarations.
    emitted_data=set();data_shells=[]
    def emit_data(name):
        if name=='ModuleData' or name in emitted_data:return
        c=classes.get(name)
        if c is None:raise ValueError(('produced data ancestry unavailable',name))
        parents=[base for base in c['bases'] if marker(base) is not None]
        if len(parents)!=1:raise ValueError(('data ancestry ambiguity',name,parents))
        emit_data(parents[0]);nearest=marker(name)
        own=nearest['code'] if nearest['owner']==name else ''
        data_shells.append('struct '+name+' : '+parents[0]+' { '+own+' };')
        emitted_data.add(name)
    for item in identities.values():emit_data(item['factory']['produced'])
    # Actual data-factory macro is copied unchanged, not replaced by a bool literal.
    macro_start=module_text.index('#define MAKE_STANDARD_MODULE_DATA_MACRO_ABC(')
    macro_end=module_text.index('\n\n',macro_start)
    macro=module_text[macro_start:macro_end]
    emitted_modules=set();module_shells=[]
    def emit_module(name):
        if name in emitted_modules:return
        c=classes.get(name)
        if c is None:raise ValueError(('factory owner hierarchy unavailable',name))
        own=definitions.get(name)
        if own is not None:
            if own['kind']=='macro':
                module_shells.append('struct '+name+' : Module { MAKE_STANDARD_MODULE_DATA_MACRO_ABC('+name+', '+own['produced']+') };')
            elif name=='Module':
                module_shells.append('struct Module { const ModuleData* getModuleData()const{return nullptr;} static ModuleData* friend_newModuleData(INI*); };\n'+module_factory)
            elif own['kind']=='inline':module_shells.append('struct '+name+' : Module { '+own['code']+' };')
            else:raise ValueError(('unsupported explicit factory owner',name))
        else:
            parents=[base for base in c['bases'] if resolve(base) is not None]
            if len(parents)!=1:raise ValueError(('static factory inheritance ambiguity',name,parents))
            emit_module(parents[0]);module_shells.append('struct '+name+' : '+parents[0]+' {};')
        emitted_modules.add(name)
    emit_module('Module')
    for family in sorted(families):emit_module(family)
    shells='\n#define MSGNEW(...) new\n'+macro+'\nnamespace FactoryIdentity {\n'+'\n'.join(data_shells)+'\n'+'\n'.join(module_shells)+'\n}\n'
    for identity in identities.values():
        for key in ['factory','marker']:
            item=identity[key]
            item['sha256']=hashlib.sha256((item.get('code') or item['association']).encode()).hexdigest()
    return global_data,shells,identities

# Drawing classes are shared Core device classes, outside the MD header subtree.
for p in (UP/'Core/GameEngineDevice/Include').rglob('*.h'):
 text=p.read_text(errors='replace')
 for m in re.finditer(r'\bclass\s+(\w+)\s*:\s*((?:public|protected|private)\s+[^\{;]+)\{',text):
  classes[m[1]]={'bases':re.findall(r'(?:public|protected|private)\s+(\w+)',m[2]),'body':block(text,m.start()),'header':str(p.relative_to(ROOT)),'line':text.count('\n',0,m.start())+1}
tp=UP/'Core/GameEngine/Source/Common/Thing/ThingTemplate.cpp';source=tp.read_text()
manifest=[]
def extract(text,symbol,path):
 start=text.index(symbol);opening=text.index('{',start);body=block(text,start);result=text[start:opening+1]+body+'}'
 manifest.append({'path':str(path.relative_to(ROOT)),'symbol':symbol,'line':text.count('\n',0,start)+1,'sha256':hashlib.sha256(result.encode()).hexdigest()})
 return result
names=['Bool ModuleInfo::clearCopiedFromDefaultEntries','Bool ModuleInfo::clearAiModuleInfo','Bool ModuleInfo::clearModuleDataWithTag','void ModuleInfo::addModuleInfo','void ThingTemplate::parseModuleName','void ThingTemplate::parseAddModule','void ThingTemplate::parseRemoveModule','void ThingTemplate::parseReplaceModule','Bool ThingTemplate::removeModuleInfo','Bool ThingTemplate::canPossiblyHaveAnyWeapon','void ThingTemplate::copyFrom','void ThingTemplate::setCopiedFromDefault']
functions='\n'.join(extract(source,n,tp) for n in names)
# Parse only structural block nesting and metadata fields. Native INI lexing,
# module-data field parsing, asset composition and object creation are not run.
opens=set('Draw Body Behavior ClientUpdate WeaponSet ArmorSet DefaultConditionState ConditionState TransitionState UnitSpecificSounds UnitSpecificFX Prerequisites AddModule ReplaceModule InheritableModule OverrideableByLikeKind AttackAreaDecal TargetingReticleDecal GridDecalTemplate DeliveryDecal'.split())
def events(name):
 o=objects[name];stack=[];out=[]
 for offset,line in enumerate(o['raw'].splitlines()):
  line=line.split(';')[0].strip();tokens=line.replace('=',' ').split()
  if not tokens:continue
  key=tokens[0]
  if key in ('Object','ObjectReskin','ChildObject'):continue
  if key.lower()=='end':
   if stack:stack.pop()
   else:break
   continue
  metadata=not stack or all(x in ('InheritableModule','OverrideableByLikeKind') for x in stack)
  if metadata and key in ('KindOf','IsTrainable'):out.append({'kind':key,'value':tokens[1:],'line':o['line']+offset})
  if metadata and key in ('Body','Behavior','Draw','ClientUpdate'):out.append({'kind':'module','family':tokens[1],'tag':tokens[2],'body':key=='Body','category':key,'mode':stack[-1] if stack else 'normal','line':o['line']+offset})
  if stack==['WeaponSet'] and key=='Weapon':out.append({'kind':'weapon','slot':tokens[1],'value':tokens[2],'line':o['line']+offset})
  if key in opens or (key in ('Turret','AltTurret') and len(tokens)==1):stack.append(key)
 return out
fixed=FIXED
ordered=[]
def visit(n):
 if n in ordered:return
 if objects[n]['parent']:visit(objects[n]['parent'])
 ordered.append(n)
for n in fixed:visit(n)
case_events={n:events(n) for n in ordered}
default_events=events('DefaultThingTemplate')
families={e['family'] for es in [default_events,*case_events.values()] for e in es if e['kind']=='module'}
# Execute native static mask function bodies with original inheritance/name calls.
# Unrelated members and interfaces are absent from these bounded class shells.
mask_defs=[];emitted=set();missing=[]
def emit_mask(name):
 if name in emitted:return
 c=classes.get(name)
 if not c:missing.append(name);return
 m=re.search(r'\bgetInterfaceMask\s*\([^)]*\)\s*\{',c['body'])
 if m:
  body=block(c['body'],m.start())
  deps=re.findall(r'(\w+)::getInterfaceMask',body)
  for dep in deps:emit_mask(dep)
  mask_defs.append('struct '+name+' { static int getInterfaceMask() {'+body+'} };')
  manifest.append({'path':c['header'],'symbol':name+'::getInterfaceMask','class_line':c['line'],'body_sha256':hashlib.sha256(body.encode()).hexdigest()})
 else:
  parents=[p for p in c['bases'] if p in classes]
  if not parents:missing.append(name);return
  emit_mask(parents[0]);mask_defs.append('struct '+name+' : '+parents[0]+' {};')
 emitted.add(name)
for name in sorted(families):emit_mask(name)
assert not missing,missing
mh=UP/'GeneralsMD/Code/GameEngine/Include/Common/Module.h';mt=mh.read_text();constants='\n'.join('constexpr int '+m[1]+'='+m[2]+';' for m in re.finditer(r'(MODULEINTERFACE_\w+)\s*=\s*(0x[0-9a-fA-F]+)',mt))
kindnames=set(re.findall(r'KINDOF_\w+',functions))|{'KINDOF_CAN_BE_REPULSED','KINDOF_SHRUBBERY'}
for es in [default_events,*case_events.values()]:
 for e in es:
  if e['kind']=='KindOf':kindnames.update('KINDOF_'+v for v in e['value'])
kindconstants='\n'.join('const std::string '+n+'="'+n.removeprefix('KINDOF_')+'";' for n in sorted(kindnames))
helper_names=['ObjectSMCHelper','StatusDamageHelper','SubdualDamageHelper','ObjectRepulsorHelper','ObjectDefectionHelper','ObjectWeaponStatusHelper','FiringTracker','TempWeaponBonusHelper']
op=UP/'GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Object.cpp';ot=op.read_text();helpercode=ot[ot.index('\t// the helpers are done first'):ot.index('\t// behaviors are always done first')]
manifest.append({'path':str(op.relative_to(ROOT)),'symbol':'Object constructor helper prefix','line':ot.count('\n',0,ot.index('\t// the helpers are done first'))+1,'sha256':hashlib.sha256(helpercode.encode()).hexdigest()})
header=r'''
#include <algorithm>
#include <cstdint>
#include <functional>
#include <iostream>
#include <map>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>
using UnsignedShort=unsigned short; using Int=int; using Bool=bool; using UnsignedInt=std::uintptr_t;
constexpr bool FALSE=false, TRUE=true;
#define RTS_GENERALS 0
#define RETAIL_COMPATIBLE_CRC 0
#define DEBUG_CRASH(x) ((void)0)
#define DEBUG_ASSERTCRASH(c,x) do { if(!(c)) throw 9; } while(0)
constexpr int INI_INVALID_DATA=17;
struct AsciiString : std::string { using std::string::string; AsciiString(const std::string&s):std::string(s){} AsciiString()=default; const char*str()const{return c_str();} bool isNotEmpty()const{return !empty();} bool isEmpty()const{return empty();} };
struct KindOfMaskType {std::set<std::string> value; void set(const std::string&s){value.insert(s);} };
struct ModuleData {bool ai=false;bool isAiModuleData()const{return ai;}};
struct ThingTemplate; struct INI;
enum ModuleType{MODULETYPE_BEHAVIOR=0,MODULETYPE_DRAW=1,MODULETYPE_CLIENT_UPDATE=2,MODULETYPE_BODY_ADAPTER=999};
enum ModuleParseMode{MODULEPARSE_NORMAL,MODULEPARSE_INHERITABLE,MODULEPARSE_OVERRIDEABLE_BY_LIKE_KIND,MODULEPARSE_ADD_REMOVE_REPLACE};
constexpr int INI_LOAD_CREATE_OVERRIDES=1;
struct ModuleInfo {
 struct Nugget {AsciiString first,m_moduleTag;const ModuleData*second;int interfaceMask;bool copiedFromDefault=false,inheritable,overrideableByLikeKind;
 Nugget(const AsciiString&n,const AsciiString&t,const ModuleData*d,int m,bool i,bool o):first(n),m_moduleTag(t),second(d),interfaceMask(m),inheritable(i),overrideableByLikeKind(o){}};
 std::vector<Nugget>m_info; void setCopiedFromDefault(bool value){for(auto&m:m_info)m.copiedFromDefault=value;}
 Bool clearCopiedFromDefaultEntries(Int,const AsciiString&,const ThingTemplate*);
 Bool clearAiModuleInfo();Bool clearModuleDataWithTag(const AsciiString&,AsciiString&);
 void addModuleInfo(ThingTemplate*,const AsciiString&,const AsciiString&,const ModuleData*,Int,Bool,Bool);
 int getCount()const{return m_info.size();} AsciiString getNthName(int n)const{return m_info[n].first;}
};
struct WeaponTemplateSet {bool armed;bool hasAnyWeapons()const{return armed;}};
using WeaponTemplateSetVector=std::vector<WeaponTemplateSet>;
struct ThingTemplate {
 ModuleInfo m_behaviorModuleInfo,m_drawModuleInfo,m_clientUpdateModuleInfo;
 ThingTemplate*m_nextThingTemplate=nullptr;UnsignedShort m_templateID=0;AsciiString m_nameString;bool m_armorCopiedFromDefault=false,m_weaponsCopiedFromDefault=false;
 void copyFrom(const ThingTemplate*);void setCopiedFromDefault();
 ModuleParseMode m_moduleParsingMode=MODULEPARSE_NORMAL;
 AsciiString m_moduleBeingReplacedName,m_moduleBeingReplacedTag;
 std::set<std::string> flags;bool trainable=false;WeaponTemplateSetVector m_weaponTemplateSets;
 bool isTrainable()const{return trainable;} bool isAnyKindOf(const KindOfMaskType&m)const{for(auto&v:m.value)if(flags.count(v))return true;return false;}
 bool isKindOf(const std::string&s)const{return flags.count(s);}
 AsciiString getName()const{return "fixture";}const void*getFieldParse()const{return nullptr;}
 Bool canPossiblyHaveAnyWeapon()const;
 Bool removeModuleInfo(const AsciiString&,AsciiString&);
 static void parseModuleName(INI*,void*,void*,const void*);
 static void parseAddModule(INI*,void*,void*,const void*);
 static void parseRemoveModule(INI*,void*,void*,const void*);
 static void parseReplaceModule(INI*,void*,void*,const void*);
};
struct INI {std::vector<std::string>tokens;unsigned index=0;int load=0;std::function<void()>nested;
 const char*getNextToken(){if(index>=tokens.size())throw INI_INVALID_DATA;return tokens[index++].c_str();}
 int getLoadType()const{return load;}int getLineNum()const{return 0;}AsciiString getFilename()const{return "fixture";}
 void initFromINI(void*,const void*){if(nested)nested();}
};
struct ModuleFactory {std::map<std::string,int>masks;std::map<std::string,ModuleData>data;
 int findModuleInterfaceMask(const AsciiString&n,ModuleType){return masks.at(n);}
 ModuleData*newModuleDataFromINI(INI*,const AsciiString&n,ModuleType,const AsciiString&){return &data.at(n);}
} factory; ModuleFactory*TheModuleFactory=&factory;
'''
helper_stub=r'''
struct AIData {bool m_enableRepulsors=true;}; struct AI {AIData data;AIData*getAiData(){return &data;}} ai; AI*TheAI=&ai;
using NameKeyType=const char*;
#define NAMEKEY(s) s
#define newInstance(T) new T
struct HelperData {void setModuleTagNameKey(const char*){}};
struct Helper {std::string family;Helper(const char*s):family(s){}virtual ~Helper()=default;};
'''
for n in helper_names:helper_stub+='struct '+n+'ModuleData:HelperData{};struct '+n+':Helper{'+n+'(void*,HelperData*):Helper("'+n+'"){} };\n'
helper_stub+='struct Object { const ThingTemplate* tt; Helper*storage[16]{}; Helper**curB=storage; AsciiString modName;\n'
for field in ['smc','statusDamage','subdualDamage','repulsor','defection','ws','firingTracker','tempWeaponBonus']:
 helper_stub+='Helper*m_'+('firingTracker' if field=='firingTracker' else field+'Helper')+'=nullptr;\n'
helper_stub+='bool isKindOf(const std::string&s)const{return tt->isKindOf(s);} std::vector<std::string>helpers(const ThingTemplate&t){tt=&t;const ModuleInfo&mi=t.m_behaviorModuleInfo;\n'+helpercode+'\nstd::vector<std::string>out;for(auto p=storage;p!=curB;++p){out.push_back((*p)->family);delete *p;}return out;} };\n'
q=json.dumps
cpp=header+constants+'\n'+kindconstants+'\n'+'\n'.join(mask_defs)+'\n'+functions+'\n'+helper_stub+r'''
void add(ThingTemplate&t,const std::string&name,const std::string&tag,int type=0,int load=0){
 INI ini;ini.tokens={name,tag};ini.load=load;auto*mi=type==1?&t.m_drawModuleInfo:type==2?&t.m_clientUpdateModuleInfo:&t.m_behaviorModuleInfo;
 ThingTemplate::parseModuleName(&ini,&t,mi,reinterpret_cast<const void*>(static_cast<std::uintptr_t>(type)));
}
std::vector<std::string>updates(const ThingTemplate&t){Object o;auto out=o.helpers(t);for(auto&m:t.m_behaviorModuleInfo.m_info)if(m.interfaceMask&MODULEINTERFACE_UPDATE)out.push_back(m.first+":"+m.m_moduleTag);return out;}
void show(const std::string&name,const std::vector<std::string>&v){std::cout<<name;for(auto&s:v)std::cout<<"\t"<<s;std::cout<<"\n";}
void mark(ThingTemplate&t){for(auto*p:{&t.m_behaviorModuleInfo,&t.m_drawModuleInfo,&t.m_clientUpdateModuleInfo})for(auto&m:p->m_info)m.copiedFromDefault=true;}
void overlay(ThingTemplate&t,const std::string&operation,const std::string&oldtag,const std::string&family,const std::string&tag){INI ini;ini.tokens={oldtag};ini.load=INI_LOAD_CREATE_OVERRIDES;ini.nested=[&](){add(t,family,tag,0,INI_LOAD_CREATE_OVERRIDES);};if(operation=="replace")ThingTemplate::parseReplaceModule(&ini,&t,nullptr,nullptr);else if(operation=="add")ThingTemplate::parseAddModule(&ini,&t,nullptr,nullptr);else ThingTemplate::parseRemoveModule(&ini,&t,nullptr,nullptr);}
int main(){
'''

global_data,factory_shells,factory_identities=build_factory_identity(classes,UP,ROOT,families,block)
cpp=cpp.replace('struct ModuleData {bool ai=false;bool isAiModuleData()const{return ai;}};',global_data)
cpp=cpp.replace('std::map<std::string,ModuleData>data;','std::map<std::string,ModuleData*>data;')
cpp=cpp.replace('return &data.at(n);','return data.at(n);')
cpp=cpp.replace('int getLoadType()const{return load;}', 'void initFromINIMultiProc(void*,void(*)(MultiIniFieldParse&)){} int getLoadType()const{return load;}')
# Macro-backed factory shells require the full INI definition; insert after the header.
position=cpp.index('constexpr int MODULEINTERFACE_UPDATE=')
cpp=cpp[:position]+factory_shells+cpp[position:]
for n in sorted(families):
 cpp+='factory.masks['+q(n)+']='+n+'::getInterfaceMask();factory.data['+q(n)+']=FactoryIdentity::'+n+'::friend_newModuleData(nullptr);\n'

def cpp_events(es,var):
 text=''
 for e in es:
  if e['kind']=='KindOf':text+=var+'.flags={'+','.join(q(v) for v in e['value'])+'};\n'
  elif e['kind']=='IsTrainable':text+=var+'.trainable='+str(e['value'][0].lower()=='yes').lower()+';\n'
  elif e['kind']=='weapon':
   if e['value'].upper()!='NONE':text+=var+'.m_weaponTemplateSets.push_back({true});\n'
  elif e['kind']=='module':
   mode={'normal':'MODULEPARSE_NORMAL','InheritableModule':'MODULEPARSE_INHERITABLE','OverrideableByLikeKind':'MODULEPARSE_OVERRIDEABLE_BY_LIKE_KIND'}[e['mode']]
   typ={'Behavior':0,'Body':999,'Draw':1,'ClientUpdate':2}[e['category']]
   text+=var+'.m_moduleParsingMode='+mode+';add('+var+','+q(e['family'])+','+q(e['tag'])+','+str(typ)+');\n'+var+'.m_moduleParsingMode=MODULEPARSE_NORMAL;\n'
 return text
cpp+='ThingTemplate defaults;\n'+cpp_events(default_events,'defaults')+'defaults.setCopiedFromDefault();std::map<std::string,ThingTemplate>installed;\n'
for name,es in case_events.items():
 parent=objects[name]['parent']
 begin='ThingTemplate t;t.copyFrom(&installed.at('+q(parent)+'));t.setCopiedFromDefault();' if parent else 'ThingTemplate t;t.copyFrom(&defaults);'
 cpp+='{'+begin+'\n'+cpp_events(es,'t')+'installed['+q(name)+']=t;show('+q(name)+',updates(t));}\n'
# Controlled cases isolate field-order and overlay behaviors; not claimed installed map syntax.
cpp+='''
{ThingTemplate t=defaults;t.flags={"VEHICLE"};t.trainable=true;add(t,"AIUpdateInterface","a");add(t,"AIUpdateInterface","b");show("controlled_ai_replacement",updates(t));}
{ThingTemplate t=defaults;t.flags={"VEHICLE"};t.trainable=true;add(t,"PhysicsBehavior","first");add(t,"PhysicsBehavior","second");overlay(t,"replace","first","PhysicsBehavior","replacement");overlay(t,"add","","PhysicsBehavior","added");show("controlled_override_append",updates(t));}
{ThingTemplate t=defaults;t.flags={"VEHICLE"};t.trainable=true;add(t,"PhysicsBehavior","first");overlay(t,"add","","StealthUpdate","explicit_map_stealth");show("controlled_override_preserves_default",updates(t));}
{ThingTemplate t=defaults;add(t,"PhysicsBehavior","early");t.flags={"VEHICLE"};t.trainable=true;add(t,"PhysicsBehavior","late");show("controlled_fields_after_first_module",updates(t));}
{ThingTemplate t=defaults;t.flags={"VEHICLE"};t.trainable=true;add(t,"PhysicsBehavior","early");show("controlled_fields_before_first_module",updates(t));}
for(int enabled=0;enabled<2;++enabled){ThingTemplate t=defaults;t.flags={"CAN_BE_REPULSED"};ai.data.m_enableRepulsors=enabled;show(enabled?"controlled_repulsor_enabled":"controlled_repulsor_disabled",updates(t));}ai.data.m_enableRepulsors=true;
{ThingTemplate t=defaults;t.flags={"SHRUBBERY"};show("controlled_shrubbery",updates(t));}
{ThingTemplate t=defaults;t.m_weaponTemplateSets={{false},{true}};show("controlled_weapon_any_set",updates(t));}
for(int action=0;action<4;++action){ThingTemplate t=defaults;add(t,"PhysicsBehavior","old");bool rejected=false;try{if(action==0)overlay(t,"replace","old","PhysicsBehavior","old");if(action==1)overlay(t,"replace","old","StealthUpdate","new");if(action==2)overlay(t,"remove","absent","","");if(action==3)add(t,"PhysicsBehavior","direct",0,INI_LOAD_CREATE_OVERRIDES);}catch(int){rejected=true;}if(!rejected)return 3;std::cout<<"expected_rejection_"<<action<<"\\tPASS\\n";}
}
'''

extra=r"""
for(const std::string family:{"TransportAIUpdate","WanderAIUpdate"}){
 ModuleData*actual=family=="TransportAIUpdate"?FactoryIdentity::TransportAIUpdate::friend_newModuleData(nullptr):FactoryIdentity::WanderAIUpdate::friend_newModuleData(nullptr);
 if(!actual->isAiModuleData())return 21;delete actual;
 for(int reverse=0;reverse<2;++reverse){ThingTemplate t=defaults;t.flags={"VEHICLE"};t.trainable=true;add(t,"PhysicsBehavior","unrelated");
  add(t,reverse?family:"AIUpdateInterface","first");add(t,reverse?"AIUpdateInterface":family,"second");
  int ais=0;for(auto&entry:t.m_behaviorModuleInfo.m_info)if(entry.second->isAiModuleData()){++ais;if(entry.m_moduleTag!="second")return 22;}if(ais!=1)return 23;
  show("controlled_inherited_factory_"+family+(reverse?"_reverse":"_forward"),updates(t));
 }
 {ThingTemplate t=defaults;t.flags={"VEHICLE"};t.trainable=true;add(t,"PhysicsBehavior","unrelated");add(t,"AIUpdateInterface","first");overlay(t,"add","",family,"second");
  int ais=0;for(auto&entry:t.m_behaviorModuleInfo.m_info)if(entry.second->isAiModuleData()){++ais;if(entry.m_moduleTag!="second")return 24;}if(ais!=1)return 25;
  show("controlled_inherited_factory_"+family+"_override_add",updates(t));}
}
std::cout<<"controlled_actual_inherited_factory_identity\tPASS\n";
"""
position=cpp.rfind('\n}')
cpp=cpp[:position]+extra+cpp[position:]


BASE.with_suffix('.cpp').write_text(cpp)
env = dict(os.environ, TMPDIR=str(out), CLANG_MODULE_CACHE_PATH=str(out/'clang-cache'))
subprocess.run([args.cxx, '-std=c++17', '-O0', '-ffp-contract=off', str(BASE.with_suffix('.cpp')), '-o', str(BASE.with_suffix('.bin'))], check=True, env=env)
native = subprocess.check_output([str(BASE.with_suffix('.bin'))], env=env)
rows = [line for line in native.decode().splitlines() if line.split('\t')[0] in case_events]
if len(rows) != 159:
    raise RuntimeError(f'Expected 159 native installed rows, got {len(rows)}')
dest = out/'test-fixtures/module-metadata'
dest.mkdir(parents=True, exist_ok=True)
payloads = {
    'installed.ini': '\n'.join(objects[n]['raw'] for n in ['DefaultThingTemplate', *case_events]).encode('latin1'),
    'installed-r2.native.tsv': ('\n'.join(rows)+'\n').encode(),
}
expected = {
    'installed.ini': '9bb583b7b88b7bdbd093ec1f9e5584246385401e3165043d28db70846b90841e',
    'installed-r2.native.tsv': '64039f6f7ec4f91157c7fa40704eeb1caf3a4c344478b40ab5f886aecbb89d8f',
}
for name, data in payloads.items():
    if hashlib.sha256(data).hexdigest() != expected[name]:
        raise RuntimeError(f'Frozen original fixture differs: {name}; check the installation/source profile')
    (dest/name).write_bytes(data)
BASE.with_suffix('.native.tsv').write_bytes(native)
if args.compare:
    for name in ('installed.ini', 'installed-r2.native.tsv'):
        if (dest/name).read_bytes() != (args.compare/'module-metadata'/name).read_bytes():
            raise RuntimeError(f'Preserved fixture differs: {name}')


def behavior(raw, family):
    text = raw.replace('\r\n', '\n')
    found = re.search(r'(?ms)^  Behavior\s*=\s*' + re.escape(family) + r'\s+[^\n]+\n.*?^  End\n', text)
    if found is None:
        raise RuntimeError(f'Missing original Behavior: {family}')
    return found[0]

hacker = objects['ChinaInfantryHacker']['raw']
small = {
    'hacker.ini': 'Object ChinaInfantryHacker\n' + behavior(hacker, 'HackInternetAIUpdate') + 'End\n',
}
power_text = assets['data/ini/specialpower.ini'].decode('latin1').replace('\r\n', '\n')
power = re.search(r'(?ms)^SpecialPower SpecialAbilityHackerDisableBuilding\n(.*?)^End\n', power_text)
if power is None:
    raise RuntimeError('Missing Hacker SpecialPower')
power_fields = {}
for line in power[1].splitlines():
    field = line.split(';', 1)[0].strip().split('=', 1)
    if len(field) == 2:
        power_fields[field[0].strip()] = field[1].strip()
small['hacker-disable.ini'] = (
    'Object ChinaInfantryHacker\n' + behavior(hacker, 'SpecialAbility') + '\n'
    + behavior(hacker, 'SpecialAbilityUpdate') + '\n\nEnd\n\n'
    + 'SpecialPower SpecialAbilityHackerDisableBuilding\n'
    + ''.join(f'  {key} = {power_fields[key]}\n' for key in ('Enum', 'ReloadTime', 'PublicTimer'))
    + 'End\n'
)
turrets = [
    'AmericaVehicleHumvee', 'AmericaVehicleMedic', 'AmericaTankCrusader',
    'ChinaTankBattleMaster', 'ChinaTankOverlord', 'ChinaTankGattling',
    'GLATankScorpion', 'GLAVehicleQuadCannon', 'GLAVehicleRadarVan', 'GLALightTank',
]
turret_text = ('; Original installed Zero Hour INIZH.big turret blocks, preserved verbatim.\n'
               '; Controlled primary turrets in the base-faction playtest roster.\n\n')
for name in turrets:
    record = objects[name]
    raw = record['raw'].replace('\r\n', '\n')
    found = re.search(r'(?ms)^ +Turret\s*\n.*?^    End\n', raw)
    if found is None:
        raise RuntimeError(f'Missing primary turret: {name}')
    path = record['asset'].replace('/', '\\')
    # Match the original diagnostic path casing without encoding retail data.
    parts = path.split('\\')
    parts[:3] = ['Data', 'INI', 'Object']
    parts[-1] = {'america': 'America', 'china': 'China', 'gla': 'GLA'}[parts[-1][:-11]] + 'Vehicle.ini'
    label = '\\'.join(parts) + ':' + str(record['line'] + raw.count('\n', 0, found.start()))
    if name == 'GLALightTank':
        turret_text += '; GLATankMarauder reskin parent. ' + label + '\n'
    else:
        turret_text += '; ' + label + '\n'
    turret_text += ('Object ' + name + '\n  Behavior = AIUpdateInterface Fixture\n'
                    + found[0] + '  End\nEnd\n\n')
small['turret-fixtures.ini'] = turret_text.rstrip('\n') + '\n'
small_expected = {
    'hacker.ini': 'e10db7d03861e3500e71a293beb438e03deaace5fbf5949e83b6ff5e8275c095',
    'hacker-disable.ini': '82301608b898a6ea7d26cd336c8bdc39f39273b0b7ca7474d198d3f480487b16',
    'turret-fixtures.ini': 'bcc9526eeadaeee7c8d3e41794a01e3e23777781280d8791c55194a299146b1e',
}
for name, text in small.items():
    data = text.encode('latin1')
    if hashlib.sha256(data).hexdigest() != small_expected[name]:
        raise RuntimeError(f'Frozen original fixture differs: {name}')
    (out/'test-fixtures'/name).write_bytes(data)
    if args.compare and data != (args.compare/name).read_bytes():
        raise RuntimeError(f'Preserved fixture differs: {name}')

print('Prepared and verified all five private installed fixtures in', out/'test-fixtures')


if args.maps_archive:
    maps = big(args.maps_archive)
    source_name = 'maps/tournament desert/tournament desert'
    map_data = maps[source_name + '.map']
    preview = maps[source_name + '.tga']
    if hashlib.sha256(map_data).hexdigest() != 'db429b884bc593971feb9d545ada1f1d0aea114f2262b052fe46d81f9f034d01':
        raise RuntimeError('Tournament Desert map differs from the frozen original')
    if hashlib.sha256(preview).hexdigest() != '7df6c361d80e15d88a37498ace4d28d3fe4038575a9d112c5e3ee668f9363f29':
        raise RuntimeError('Tournament Desert preview differs from the frozen original')
    cache = maps['maps/mapcache.ini'].decode('latin1').replace('\r\n', '\n')
    record = re.search(r'(?ms)^MapCache maps_5Ctournament_20desert_5Ctournament_20desert_2Emap\n(.*?)^END\s*$', cache, re.I)
    if record is None:
        raise RuntimeError('Missing original Tournament Desert MapCache metadata')
    fields = record[1]
    size = re.search(r'(?mi)^\s*fileSize\s*=\s*(\d+)', fields)
    if size is None or int(size[1]) != len(map_data):
        raise RuntimeError('Original Tournament Desert MapCache size does not match map')
    # Unchanged map bytes retain the original archive's CRC and coordinate records.
    fields = re.sub(r'(?mi)^(\s*isOfficial\s*=)[^\n]*', r'\1 no', fields)
    fields = re.sub(r'(?mi)^(\s*nameLookupTag\s*=)[^\n]*', r'\1', fields)
    custom = out/'UserData/Maps'
    custom.mkdir(parents=True)
    records = []
    for index in range(1, 13):
        name = f'Custom Desert {index:02}'
        directory = custom/name
        directory.mkdir()
        path = directory/(name+'.map')
        path.write_bytes(map_data)
        (directory/(name+'.tga')).write_bytes(preview)
        encoded = ''.join(chr(byte) if chr(byte).isascii() and chr(byte).isalnum() else f'_{byte:02X}' for byte in str(path).encode())
        records.append('MapCache '+encoded+'\n'+fields+'End\n')
    (custom/'MapCache.ini').write_text('\n'.join(records))
    print('Prepared 12 unchanged Tournament Desert map/preview copies and current-path MapCache in', custom)
