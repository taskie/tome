import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  workspaceName: string;
  groupName: string;
  family?: string;
};

export const GroupLink: React.FC<Props> = ({ workspaceName, groupName, family }) => {
  const href =
    family === "stats" ? uri`/${workspaceName}/stats/${groupName}` : uri`/${workspaceName}/groups/${groupName}`;
  return (
    <Link href={href}>
      <a>{groupName}</a>
    </Link>
  );
};

export default GroupLink;
