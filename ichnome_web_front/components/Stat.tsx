import { IchStat } from "@/api/types";
import GroupLink from "./GroupLink";
import StatLink from "./StatLink";
import FootprintLink from "./FootprintLink";

type Props = {
  stat: IchStat;
};

export const Stat: React.FC<Props> = ({
  stat: { group_id, path, version, status, mtime, digest, size, created_at, updated_at },
}) => {
  return (
    <ul>
      <li>
        Group: <GroupLink groupId={group_id} />
      </li>
      <li>
        Path: <StatLink groupId={group_id} path={path} />
      </li>
      <li>Version: {version}</li>
      <li>Status: {status}</li>
      {mtime != null ? <li>File Modified At: {mtime}</li> : undefined}
      {digest != null ? (
        <li>
          Digest: <FootprintLink digest={digest} />
        </li>
      ) : undefined}
      {size != null ? <li>Size: {size}</li> : undefined}
      <li>Stat Created At: {created_at}</li>
      <li>Stat Updated At: {updated_at}</li>
    </ul>
  );
};

export default Stat;
